//! Capture never waits for inference. The tap enqueues bounded device-rate
//! frames; a separate task resamples, talks to the persistent sidecar, and
//! hands a coverage-checked final to the matching recording generation.
use super::messages::{encode_audio, Command, NativeTranscript, Response};
use super::pcm::PcmConverter;
use super::CrispasrManager;
use crate::audio::stream_tap::{StreamTapSink, StreamTapSinkFactory};
use crate::transcription::stream::{StreamSessionGate, TranscriptionStreamEvent};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, oneshot};

enum Control {
    Audio(Vec<i16>),
    Finalize(u64),
}

struct Sink {
    sender: mpsc::Sender<Control>,
    cancelled: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    max_pending: usize,
    finalized: bool,
}

impl StreamTapSink for Sink {
    fn send_frame(&mut self, samples: &[i16]) {
        if samples.is_empty()
            || self.cancelled.load(Ordering::SeqCst)
            || self.stopped.load(Ordering::SeqCst)
        {
            return;
        }
        let previous = self.pending.fetch_add(samples.len(), Ordering::SeqCst);
        if previous.saturating_add(samples.len()) > self.max_pending {
            self.pending.fetch_sub(samples.len(), Ordering::SeqCst);
            self.cancelled.store(true, Ordering::SeqCst);
            return;
        }
        if self
            .sender
            .try_send(Control::Audio(samples.to_vec()))
            .is_err()
        {
            self.pending.fetch_sub(samples.len(), Ordering::SeqCst);
            self.cancelled.store(true, Ordering::SeqCst);
        }
    }
    fn finalize(&mut self, dropped_frames: u64) -> Option<String> {
        self.stopped.store(true, Ordering::SeqCst);
        self.finalized = true;
        if self
            .sender
            .try_send(Control::Finalize(dropped_frames))
            .is_err()
        {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        // The generation-keyed oneshot is awaited by transcription, not by the
        // recorder's bounded join. No decode gets added to the stop budget.
        None
    }
    fn cancel(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.cancelled.store(true, Ordering::SeqCst);
    }
}
impl Drop for Sink {
    fn drop(&mut self) {
        if !self.finalized {
            self.cancel();
        }
    }
}

struct Session {
    app: AppHandle,
    model: String,
    language: String,
    generation: u64,
    preview: bool,
    sample_rate: u32,
    channels: u16,
    cancelled: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
}

pub fn factory(
    app: AppHandle,
    model: String,
    language: String,
    generation: u64,
    preview: bool,
) -> StreamTapSinkFactory {
    Arc::new(move |sample_rate, channels| {
        let max_pending = sample_rate as usize * channels as usize * 5;
        // Every data message contains at least one sample. This message ceiling
        // cannot shorten the five-second audio budget on small-callback devices;
        // one extra slot is reserved for finalization.
        let (sender, receiver) = mpsc::channel(max_pending.saturating_add(1));
        let (final_sender, final_receiver) = oneshot::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let manager = app.try_state::<CrispasrManager>()?;
        if !manager.finals.register(
            generation,
            model.clone(),
            language.clone(),
            final_receiver,
            cancelled.clone(),
        ) {
            return None;
        }
        let stopped = Arc::new(AtomicBool::new(false));
        let pending = Arc::new(AtomicUsize::new(0));
        let session = Session {
            app: app.clone(),
            model: model.clone(),
            language: language.clone(),
            generation,
            preview,
            sample_rate,
            channels,
            cancelled: cancelled.clone(),
            stopped: stopped.clone(),
            pending: pending.clone(),
        };
        tauri::async_runtime::spawn(async move {
            let result = session.run(receiver).await;
            let _ = final_sender.send(result);
        });
        Some(Box::new(Sink {
            sender,
            cancelled,
            stopped,
            pending,
            max_pending,
            finalized: false,
        }) as Box<dyn StreamTapSink>)
    })
}

impl Session {
    async fn run(self, mut receiver: mpsc::Receiver<Control>) -> Result<NativeTranscript, String> {
        let manager = self.app.state::<CrispasrManager>();
        let app_cancel = self
            .app
            .state::<crate::state::AppState>()
            .should_cancel_recording
            .clone();
        let cancelled = || {
            self.cancelled.load(Ordering::SeqCst)
                || app_cancel.load(Ordering::SeqCst)
                || manager.is_shutting_down()
                || crate::commands::audio::current_recording_generation() != self.generation
        };
        if cancelled() {
            return Err("Transcription cancelled".into());
        }
        let mut converter = PcmConverter::new(self.sample_rate, self.channels)?;
        let gate = Arc::new(Mutex::new(StreamSessionGate::new(self.generation)));
        let revision = AtomicU64::new(0);
        if self.preview {
            crate::commands::audio::emit_stream_event(
                &self.app,
                &gate,
                TranscriptionStreamEvent::Started {
                    session_id: self.generation,
                    engine: "crispasr".into(),
                    revision: 0,
                },
            );
        }
        let mut committed = String::new();
        let mut on_partial = |session_id, next: String, tentative: String| {
            if !self.preview
                || self.stopped.load(Ordering::SeqCst)
                || cancelled()
                || session_id != self.generation
            {
                return;
            }
            if !StreamSessionGate::assert_committed_monotonic(&committed, &next) {
                return;
            }
            committed = next.clone();
            crate::commands::audio::emit_stream_event(
                &self.app,
                &gate,
                TranscriptionStreamEvent::Partial {
                    session_id,
                    revision: revision.fetch_add(1, Ordering::SeqCst) + 1,
                    committed: next,
                    tentative,
                },
            );
        };
        let mut runtime = manager.runtime.lock().await;
        let mut outcome = async {
            manager.ensure_loaded(&self.app, &mut runtime, &self.model, &cancelled, false).await?;
            let process = runtime.process.as_mut().ok_or("CrispASR runtime unavailable")?;
            process.request(Command::StartStream { session_id: self.generation, language: &self.language, mode: "recording" }, Duration::from_secs(10), &cancelled, &mut on_partial).await?;
            let mut sent = 0u64;
            loop {
                if cancelled() { return Err("Transcription cancelled".into()); }
                let command = tokio::select! {
                    command = receiver.recv() => command.ok_or("CrispASR stream disconnected")?,
                    _ = tokio::time::sleep(Duration::from_millis(20)) => continue,
                };
                let (samples, finalizing) = match command {
                    Control::Audio(samples) => {
                        // Includes the in-flight chunk in the backlog until its
                        // native acknowledgment; slow inference cannot hide lag.
                        let count = samples.len();
                        let output = converter.push(&samples)?;
                        (output, Some(count))
                    }
                    Control::Finalize(dropped) => {
                        if dropped != 0 { return Err("CrispASR stream missed capture frames".into()); }
                        let output = std::mem::replace(&mut converter, PcmConverter::new(16_000, 1)?).finish()?;
                        (output, None)
                    }
                };
                for chunk in samples.chunks(16_000) {
                    let pcm = encode_audio(chunk);
                    let response = process.request(Command::AudioChunk { session_id: self.generation, pcm: &pcm }, Duration::from_secs(60), &cancelled, &mut on_partial).await?;
                    sent += chunk.len() as u64;
                    if !matches!(response, Response::Ok { samples: Some(count), .. } if count == sent) { return Err("CrispASR stream audio coverage mismatch".into()); }
                }
                if let Some(count) = finalizing {
                    self.pending.fetch_sub(count, Ordering::SeqCst);
                } else {
                    let budget = crate::transcription::engines::transcription_watchdog_budget(Some(sent * 1000 / 16_000));
                    return match process.request(Command::FinalizeStream { session_id: self.generation }, budget, &cancelled, &mut on_partial).await? {
                        Response::Final { session_id, result, .. } if session_id == self.generation && result.samples == sent => Ok(result),
                        _ => Err("CrispASR final stream coverage mismatch".into()),
                    };
                }
            }
        }.await;
        if cancelled() {
            outcome = Err("Transcription cancelled".into());
        }
        if outcome.is_err() {
            if let Some(mut process) = runtime.process.take() {
                process.abort().await;
            }
        } else {
            runtime.record_success(&self.model);
        }
        if self.preview && crate::commands::audio::current_recording_generation() == self.generation
        {
            let event = match &outcome {
                Ok(result) => TranscriptionStreamEvent::Final {
                    session_id: self.generation,
                    revision: revision.fetch_add(1, Ordering::SeqCst) + 1,
                    text: result.text.clone(),
                },
                Err(_) => TranscriptionStreamEvent::Cancelled {
                    session_id: self.generation,
                    revision: revision.fetch_add(1, Ordering::SeqCst) + 1,
                },
            };
            crate::commands::audio::emit_stream_event(&self.app, &gate, event);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finalization_follows_every_queued_frame_and_carries_capture_loss() {
        let (sender, mut receiver) = mpsc::channel(3);
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut sink = Sink {
            sender,
            cancelled: cancelled.clone(),
            stopped: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(AtomicUsize::new(0)),
            max_pending: 4,
            finalized: false,
        };
        sink.send_frame(&[1, 2]);
        sink.send_frame(&[3, 4]);
        assert!(sink.finalize(7).is_none());
        sink.send_frame(&[5]); // Capture after terminal stop must not extend the take.
        drop(sink);
        assert!(matches!(receiver.try_recv(), Ok(Control::Audio(frame)) if frame == [1, 2]));
        assert!(matches!(receiver.try_recv(), Ok(Control::Audio(frame)) if frame == [3, 4]));
        assert!(matches!(receiver.try_recv(), Ok(Control::Finalize(7))));
        assert!(receiver.try_recv().is_err());
        assert!(!cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn backlog_overflow_invalidates_instead_of_silently_dropping_audio() {
        let (sender, _receiver) = mpsc::channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut sink = Sink {
            sender,
            cancelled: cancelled.clone(),
            stopped: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(AtomicUsize::new(0)),
            max_pending: 2,
            finalized: false,
        };
        sink.send_frame(&[1, 2, 3]);
        assert!(cancelled.load(Ordering::SeqCst));
        assert_eq!(sink.pending.load(Ordering::SeqCst), 0);
    }
}
