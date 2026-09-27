//! Whisper decode-ahead live preview — the threading bridge (plan 032, Claude slice).
//!
//! A synchronous [`WhisperDecodeStreamHandle`] (used by the recorder tap on its worker
//! thread) feeds a dedicated **std::thread** that runs the blocking `whisper_full`
//! decode-ahead loop. `send_chunk` only enqueues (never blocks the recorder), mirroring
//! `ParakeetStreamHandle`. The thread converts device-rate i16 frames → 16 kHz mono f32
//! (reusing the plan-049 [`StreamingResampler`]), pushes them into the pure
//! [`DecodeAheadBuffer`], and — when the buffer says so — re-decodes the growing window,
//! emitting committed/tentative preview text. On finalize it does one eos decode and
//! returns the committed total (preview only; the authoritative text is still the batch
//! decode at stop).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::audio::resampler::StreamingResampler;
use crate::whisper::decode_ahead::{DecodeAheadBuffer, DecodeAheadPartial};
use crate::whisper::transcriber::Transcriber;

/// Whisper's required input rate.
const TARGET_RATE: u32 = 16_000;
/// Hard cap on the final eos decode; on expiry the preview `Final` is skipped (the
/// authoritative batch decode at stop is unaffected).
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(8);

enum Control {
    Chunk(Vec<i16>),
    Finalize,
    Cancel,
}

/// Parameters for a preview decode session.
pub(crate) struct WhisperStreamConfig {
    /// The already-loaded model, shared from the `TranscriberCache` (Arc clone — no
    /// second model load; a fresh `WhisperState` per decode keeps the batch path safe).
    pub transcriber: Arc<Transcriber>,
    pub input_sample_rate: u32,
    pub channels: u16,
    /// Preview language (English default; auto is not offered).
    pub language: Option<String>,
}

/// Sync handle over the decode thread. Mirrors `ParakeetStreamHandle`.
pub(crate) struct WhisperDecodeStreamHandle {
    tx: mpsc::Sender<Control>,
    final_rx: mpsc::Receiver<String>,
    /// Set synchronously by finalize/cancel/drop so the decode thread stops emitting
    /// preview partials the instant stop is requested — even for a `whisper_full` already
    /// in flight or chunks still queued. Prevents stale same-session partials reaching the
    /// pill after the recording has stopped.
    stopped: Arc<AtomicBool>,
}

impl WhisperDecodeStreamHandle {
    /// Enqueue a device-rate PCM chunk. Non-blocking.
    pub(crate) fn send_chunk(&self, samples: &[i16]) {
        let _ = self.tx.send(Control::Chunk(samples.to_vec()));
    }

    /// Request end-of-stream and return the committed preview text. Returns immediately
    /// (no extra decode — the authoritative full text is the batch decode at stop).
    pub(crate) fn finalize(&self) -> Option<String> {
        self.stopped.store(true, Ordering::SeqCst);
        if self.tx.send(Control::Finalize).is_err() {
            return None;
        }
        self.final_rx.recv_timeout(FINALIZE_TIMEOUT).ok()
    }

    pub(crate) fn cancel(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Control::Cancel);
    }
}

impl Drop for WhisperDecodeStreamHandle {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Control::Cancel);
    }
}

/// Spawn the decode thread; returns the handle immediately (never blocks the caller).
pub(crate) fn open<F>(config: WhisperStreamConfig, on_partial: F) -> WhisperDecodeStreamHandle
where
    F: Fn(DecodeAheadPartial) + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let (final_tx, final_rx) = mpsc::channel();
    let stopped = Arc::new(AtomicBool::new(false));
    let stopped_for_thread = stopped.clone();
    thread::spawn(move || run_decode_thread(config, rx, final_tx, on_partial, stopped_for_thread));
    WhisperDecodeStreamHandle {
        tx,
        final_rx,
        stopped,
    }
}

/// Convert an interleaved device-rate i16 chunk to 16 kHz mono f32, feeding the
/// stateful resampler. Returns an empty vec on resampler error (logged) so one bad
/// chunk never kills the preview.
fn to_target_mono_f32(
    samples: &[i16],
    channels: u16,
    resampler: &mut Option<StreamingResampler>,
) -> Vec<f32> {
    let channels = channels.max(1) as usize;
    // i16 -> f32 in [-1, 1); downmix interleaved frames to mono by averaging channels.
    let mono: Vec<f32> = if channels == 1 {
        samples.iter().map(|s| *s as f32 / 32768.0).collect()
    } else {
        samples
            .chunks(channels)
            .map(|frame| {
                frame.iter().map(|s| *s as f32 / 32768.0).sum::<f32>() / frame.len() as f32
            })
            .collect()
    };
    match resampler {
        Some(r) => match r.push(&mono) {
            Ok(out) => out,
            Err(error) => {
                log::warn!("Whisper preview resample failed: {error}");
                Vec::new()
            }
        },
        None => mono,
    }
}

fn run_decode_thread<F>(
    config: WhisperStreamConfig,
    rx: mpsc::Receiver<Control>,
    final_tx: mpsc::Sender<String>,
    on_partial: F,
    stopped: Arc<AtomicBool>,
) where
    F: Fn(DecodeAheadPartial) + Send + 'static,
{
    let mut buffer = DecodeAheadBuffer::with_default_config();
    let mut resampler = if config.input_sample_rate != TARGET_RATE {
        match StreamingResampler::new(config.input_sample_rate as usize) {
            Ok(r) => Some(r),
            Err(error) => {
                log::warn!("Whisper preview resampler init failed ({error}); preview disabled");
                None
            }
        }
    } else {
        None
    };
    let language = config.language.as_deref();

    while let Ok(control) = rx.recv() {
        match control {
            Control::Chunk(samples) => {
                let target = to_target_mono_f32(&samples, config.channels, &mut resampler);
                if !target.is_empty() {
                    buffer.push(&target);
                }
                // Stop requested (finalize/cancel/drop): buffer only, never start a new
                // decode or emit a partial once the recording is over.
                if stopped.load(Ordering::SeqCst) {
                    continue;
                }
                if buffer.should_decode(false) {
                    match config.transcriber.decode_window(buffer.window(), language) {
                        Ok(segments) => {
                            let partial = buffer.ingest(&segments, false);
                            // Re-check AFTER the (slow) decode: if stop landed while this
                            // whisper_full ran, drop the now-stale same-session partial.
                            if !stopped.load(Ordering::SeqCst) {
                                on_partial(partial);
                            }
                        }
                        Err(error) => log::warn!("Whisper preview decode failed: {error}"),
                    }
                }
            }
            Control::Finalize => {
                // Preview Final = committed-so-far. Deliberately no eos decode here: it
                // would delay stop teardown and duplicate the authoritative batch decode,
                // which already covers the uncommitted tail.
                let _ = final_tx.send(buffer.committed().to_string());
                return;
            }
            Control::Cancel => return,
        }
    }
}
