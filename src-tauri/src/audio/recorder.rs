use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

use super::level_meter::AudioLevelMeter;
use super::silence_detector::{SilenceDetector, SilenceDetectorEvent};
use super::stream_tap::{self, StreamTapRt, StreamTapSinkFactory, StreamTapWorkerSummary};

// Type-safe recording size limits
pub struct RecordingSize;

impl RecordingSize {
    const MAX_RECORDING_SIZE: u64 = 500 * 1024 * 1024; // 500MB max for recordings

    pub fn check(size: u64) -> Result<(), String> {
        if size > Self::MAX_RECORDING_SIZE {
            return Err(format!(
                "Recording size {} bytes ({:.1}MB) exceeds maximum of 500MB",
                size,
                size as f64 / 1024.0 / 1024.0
            ));
        }
        Ok(())
    }
}
const WRITER_QUEUE_CAPACITY: usize = 1024; // Bounded memory; multi-second disk-stall slack.

// Minimum capacity (in i16 samples) of each recycled chunk and each scratch
// conversion buffer. Real audio devices never deliver per-callback buffers this
// large, so this floor guarantees a reused buffer can swallow any realistic
// callback payload without growing — growing would allocate on the real-time
// audio thread and glitch the capture.
const CHUNK_CAPACITY_MIN: usize = 8192;
// Fallback per-callback frame bound used when a host reports no buffer-size
// range for its default input config (some ALSA/OSS setups). 8192 frames
// comfortably exceeds the largest buffer real input devices expose.
const CHUNK_FALLBACK_FRAMES: usize = 8192;
// Buffers preallocated beyond the writer-queue depth so one is always free while
// another is being written and a third is being assembled in the callback. By
// conservation this guarantees the recycle channel is never empty during capture:
// the RT callback only ever reuses a buffer — it never allocates (plan 008).
const CHUNK_POOL_SLACK: usize = 4;
// Capacity of the recycle (free-pool) channel. It must hold the ENTIRE
// preallocated pool so returning a buffer never blocks and never heap-allocates:
// a bounded `sync_channel` is backed by a fixed ring buffer (no per-send node,
// unlike an unbounded `channel`), keeping the RT callback's queue-full drop path
// alloc-free. By conservation the channel can never contain more than
// `WRITER_QUEUE_CAPACITY + CHUNK_POOL_SLACK` buffers at once, so this exact bound
// makes `try_send` on the real-time thread always succeed without blocking.
const RECYCLE_CHANNEL_CAPACITY: usize = WRITER_QUEUE_CAPACITY + CHUNK_POOL_SLACK;

/// Hard deadline the recording thread gives the WAV writer worker to drain its
/// bounded queue and finalize during stop. With the block-write fast path the
/// writer finishes in microseconds; this is pure defense against a writer
/// stalled on a pathological filesystem. Must stay small enough that
/// [`STOP_JOIN_TIMEOUT`] comfortably covers it plus the drain + platform
/// stream-drop budgets.
const WRITER_JOIN_TIMEOUT: Duration = Duration::from_secs(3);
const STREAM_TAP_JOIN_TIMEOUT: Duration = Duration::from_millis(100);

/// Outer budget [`AudioRecorder::stop_recording`] gives the whole recording
/// thread to tear down during stop. It must cover every internal sub-budget —
/// post-roll (250ms) + drain window (~200ms) + platform stream drop (≤3s on Windows) + writer
/// finalize ([`WRITER_JOIN_TIMEOUT`]) + stream-tap observation
/// ([`STREAM_TAP_JOIN_TIMEOUT`]) — plus a finalize margin. It is
/// intentionally larger than their sum so the outer join never preempts the
/// worker while it is still finalizing the WAV; the old independent 5s poll
/// raced the (then-unbounded) writer join and timed out mid-finalize, leaving
/// an unfinalized WAV that the command layer then deleted.
const STOP_JOIN_TIMEOUT: Duration = Duration::from_secs(8);

/// Keep capturing the end of a word after a user stops recording.
pub const STOP_POST_ROLL: Duration = Duration::from_millis(250);

enum WriterMsg {
    Chunk(Vec<i16>),
    Finalize,
}

/// One writer-loop step, factored out so the disconnect-independent finalize
/// path is unit-testable.
#[derive(Debug)]
enum WriterAction {
    Write(Vec<i16>),
    Stop,
    Idle,
}

/// Maps a `recv_timeout` outcome + the finalize flag to the writer's next step.
/// The `Timeout + finalize_requested => Stop` arm is the disconnect-independent
/// finalize: it fires even when a callback `writer_tx` clone keeps the channel
/// alive (Windows stream-drop timeout) so a `Full` `Finalize` never arrives and
/// the channel never disconnects.
fn next_writer_action(
    recv: Result<WriterMsg, mpsc::RecvTimeoutError>,
    finalize_requested: bool,
) -> WriterAction {
    match recv {
        Ok(WriterMsg::Chunk(samples)) => WriterAction::Write(samples),
        Ok(WriterMsg::Finalize) => WriterAction::Stop,
        Err(mpsc::RecvTimeoutError::Disconnected) => WriterAction::Stop,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            if finalize_requested {
                WriterAction::Stop
            } else {
                WriterAction::Idle
            }
        }
    }
}

/// One final-callback drain step (plan 060.1), factored out so the evidence +
/// integrity guarantees are unit-testable.
///
/// The stop boundary usually lands right after the last word, so the final
/// buffer MUST feed the shared capture speech evidence — otherwise the
/// no-speech gate can delete a recording whose speech the metrics never saw.
/// And a final buffer that cannot reach the writer must be COUNTED as
/// dropped, so the writer's integrity check fails the recording instead of
/// presenting a silently truncated WAV (with clean-looking evidence) as
/// complete. Single traversal, atomics only, no allocation — runs on the
/// real-time callback thread.
#[allow(clippy::too_many_arguments)]
fn drain_final_callback(
    f32_samples: &[f32],
    i16_samples: &[i16],
    capture_metrics: &CaptureMetricsAccumulator,
    recycle_rx: &mpsc::Receiver<Vec<i16>>,
    writer_tx: &SyncSender<WriterMsg>,
    recycle_tx: &SyncSender<Vec<i16>>,
    dropped_chunks: &AtomicU64,
    stream_tap_rt: Option<&StreamTapRt>,
) {
    // Evidence contribution shares the steady-state path's single traversal.
    // Level meter / silence detector stay steady-state-only: they drive live
    // UI, not evidence, and the stream is closing anyway.
    capture_metrics.observe(f32_samples);
    if let Some(tap) = stream_tap_rt {
        stream_tap::enqueue_frame_rt(tap, i16_samples);
    }

    let Ok(mut chunk) = recycle_rx.try_recv() else {
        // Pool exhausted: the final buffer cannot be written — count it so
        // the integrity check fails the recording (never silently truncate).
        dropped_chunks.fetch_add(1, Ordering::SeqCst);
        return;
    };
    chunk.clear();
    chunk.extend_from_slice(i16_samples);
    match writer_tx.try_send(WriterMsg::Chunk(chunk)) {
        Ok(()) => {}
        Err(TrySendError::Full(WriterMsg::Chunk(mut chunk))) => {
            dropped_chunks.fetch_add(1, Ordering::SeqCst);
            chunk.clear();
            // Full carries the message we tried to send (never the queue
            // front). Return that buffer to the pool for the RT thread;
            // the drop is counted above.
            let _ = recycle_tx.try_send(chunk);
        }
        // Disconnected (writer gone) counts the same way. The
        // Full(Finalize) arm exists only for pattern totality: a Chunk
        // send can never observe a Full carrying a different message.
        Err(TrySendError::Full(WriterMsg::Finalize)) | Err(TrySendError::Disconnected(_)) => {
            dropped_chunks.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn f32_to_i16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    (clamped * 32767.0) as i16
}

fn i16_to_f32(sample: i16) -> f32 {
    sample as f32 / i16::MAX as f32
}

fn u16_to_f32(sample: u16) -> f32 {
    (sample as f32 - 32768.0) / 32768.0
}

fn u16_to_i16(sample: u16) -> i16 {
    (sample as i32 - 32768) as i16
}

/// Upper bound on a *plausible* per-callback frame count. CPAL's WASAPI backend
/// reports `SupportedBufferSize::Range { max: u32::MAX }` for the normal software
/// audio stack (its `GetBufferSizeLimits` only succeeds for hardware-offloaded
/// audio). Trusting that value made the chunk-pool preallocation below request
/// terabytes of RAM and ABORT the process on Windows (silent crash, no panic).
/// Real input callbacks deliver at most a few thousand frames, so any reported
/// max above this ceiling is treated as "unknown" and falls back to a safe default.
const MAX_REASONABLE_CALLBACK_FRAMES: usize = 1 << 16; // 65536 frames (~1.3s @ 48kHz)

/// Sanitizes a device-reported max buffer size (in frames) into a trustworthy
/// bound, rejecting bogus/unbounded values (0, or above
/// [`MAX_REASONABLE_CALLBACK_FRAMES`]) so the chunk pool never preallocates from a
/// garbage size. Returns `None` (→ caller falls back) for rejected values.
fn sane_max_frames(reported_max: u32) -> Option<usize> {
    let frames = reported_max as usize;
    (1..=MAX_REASONABLE_CALLBACK_FRAMES)
        .contains(&frames)
        .then_some(frames)
}

/// Finds the named input device. The system default is checked first: the chosen
/// mic usually is the default, and that skips a full enumeration (~40 ms on
/// macOS) before the mic can open.
pub(crate) fn find_input_device(
    host: &cpal::Host,
    name: &str,
) -> Result<Option<cpal::Device>, cpal::DevicesError> {
    let named = |device: &cpal::Device| device.name().is_ok_and(|n| n == name);
    if let Some(device) = host.default_input_device().filter(named) {
        return Ok(Some(device));
    }
    Ok(host.input_devices()?.find(named))
}

/// Largest callback payload (in i16 samples) a CPAL input stream built from
/// `config` could ever deliver on the real-time thread.
///
/// CPAL always delivers exactly one buffer per callback, and the host can only
/// pick a frame count inside the device's supported buffer-size range, so the
/// range maximum (times the channel count) bounds every callback's sample count.
/// Sizing every preallocated chunk — and the scratch conversion buffers — to this
/// bound means `extend_from_slice` on the real-time thread never reallocates.
fn max_callback_samples(device: &cpal::Device, config: &cpal::SupportedStreamConfig) -> usize {
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let max_frames = device
        .supported_input_configs()
        .ok()
        .into_iter()
        .flatten()
        .filter(|c| c.channels() == config.channels() && c.sample_format() == format)
        .filter_map(|c| match c.buffer_size() {
            cpal::SupportedBufferSize::Range { max, .. } => sane_max_frames(*max),
            cpal::SupportedBufferSize::Unknown => None,
        })
        .max()
        .unwrap_or(CHUNK_FALLBACK_FRAMES);
    chunk_capacity_for(max_frames, channels)
}

/// Pure core of [`max_callback_samples`]: the capacity (in i16 samples) needed so
/// a reused buffer never grows for a callback delivering `max_frames` frames
/// across `channels`. Floored at [`CHUNK_CAPACITY_MIN`] so a device that
/// under-reports its range still leaves headroom.
fn chunk_capacity_for(max_frames: usize, channels: usize) -> usize {
    max_frames.saturating_mul(channels).max(CHUNK_CAPACITY_MIN)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureAudioMetrics {
    pub sample_count: u64,
    pub start_to_first_audio_ms: Option<u64>,
    pub duration_ms: u64,
    pub rms: f64,
    pub peak: f32,
    /// Highest single-callback (window) RMS seen during the capture. A short
    /// quiet word inside a long silent capture keeps the aggregate RMS low
    /// but pushes this well above room tone — the no-speech gate's guard
    /// against deleting real speech.
    pub max_window_rms: f32,
    /// How many milliseconds of audio sat in callback windows above the
    /// no-speech floor. Real speech spans >= tens of ms above it; a mic
    /// wake-up pop or click is a few ms. Duration — not callback count —
    /// so the discriminator is independent of the device buffer size.
    pub ms_above_rms_floor: u64,
    /// How many callback windows exceeded the no-speech floor (telemetry
    /// only; decisions use `ms_above_rms_floor`).
    pub windows_above_rms_floor: u32,
    pub sample_rate: u32,
    pub channels: u16,
    pub speech_detected: bool,
    pub post_roll_ms: u64,
    pub post_roll_interrupted: bool,
    /// A callback exceeded the silence detector's voice level during post-roll.
    pub post_roll_speech_detected: bool,
}

const SPEECH_EVIDENCE_WINDOW_MS: u64 = 5;

#[derive(Debug)]
struct CaptureMetricsAccumulator {
    sample_count: AtomicU64,
    start_to_first_audio_ms: AtomicU64,
    sum_squares_bits: AtomicU64,
    peak_bits: AtomicU32,
    max_window_rms_bits: AtomicU32,
    evidence_window_samples: u64,
    evidence_window_sum_squares_bits: AtomicU64,
    evidence_window_sample_count: AtomicU64,
    samples_above_rms_floor: AtomicU64,
    windows_above_rms_floor: AtomicU32,
    post_roll_active: AtomicBool,
    post_roll_speech_detected: AtomicBool,
    post_roll_ms: AtomicU64,
    post_roll_interrupted: AtomicBool,
}

impl CaptureMetricsAccumulator {
    fn new(sample_rate: u32, channels: u16) -> Self {
        let frames_per_window = u64::from(sample_rate.max(1))
            .saturating_mul(SPEECH_EVIDENCE_WINDOW_MS)
            .checked_div(1000)
            .unwrap_or(0)
            .max(1);
        Self {
            sample_count: AtomicU64::new(0),
            start_to_first_audio_ms: AtomicU64::new(u64::MAX),
            sum_squares_bits: AtomicU64::new(0),
            peak_bits: AtomicU32::new(0),
            max_window_rms_bits: AtomicU32::new(0),
            evidence_window_samples: frames_per_window.saturating_mul(u64::from(channels.max(1))),
            evidence_window_sum_squares_bits: AtomicU64::new(0),
            evidence_window_sample_count: AtomicU64::new(0),
            samples_above_rms_floor: AtomicU64::new(0),
            windows_above_rms_floor: AtomicU32::new(0),
            post_roll_active: AtomicBool::new(false),
            post_roll_speech_detected: AtomicBool::new(false),
            post_roll_ms: AtomicU64::new(0),
            post_roll_interrupted: AtomicBool::new(false),
        }
    }

    /// Observe one CPAL callback buffer and return its RMS. Recording-wide
    /// metrics and fixed 10ms evidence windows share this single sample traversal.
    fn observe(&self, samples: &[f32]) -> f32 {
        let mut callback_sum_squares = 0.0f32;
        let mut aggregate_sum_squares = 0.0f64;
        let mut peak = 0.0f32;
        let mut evidence_sum_squares = f64::from_bits(
            self.evidence_window_sum_squares_bits
                .load(Ordering::Relaxed),
        );
        let mut evidence_sample_count = self.evidence_window_sample_count.load(Ordering::Relaxed);
        let mut samples_above_floor = 0u64;
        let mut windows_above_floor = 0u32;

        for &sample in samples {
            callback_sum_squares += sample * sample;
            let sample_f64 = sample as f64;
            let sample_square = sample_f64 * sample_f64;
            aggregate_sum_squares += sample_square;
            peak = peak.max(sample.abs());

            evidence_sum_squares += sample_square;
            evidence_sample_count += 1;
            if evidence_sample_count == self.evidence_window_samples {
                let evidence_rms =
                    (evidence_sum_squares / evidence_sample_count as f64).sqrt() as f32;
                if evidence_rms > crate::audio::speech_evidence::NO_SPEECH_WINDOW_RMS_FLOOR {
                    samples_above_floor = samples_above_floor.saturating_add(evidence_sample_count);
                    windows_above_floor = windows_above_floor.saturating_add(1);
                }
                evidence_sum_squares = 0.0;
                evidence_sample_count = 0;
            }
        }

        // Bit-exact legacy formula (including NaN on empty slices) — the
        // silence-detector threshold math depends on these bits.
        let window_rms = (callback_sum_squares / samples.len() as f32).sqrt();
        if self.post_roll_active.load(Ordering::SeqCst)
            && window_rms > crate::audio::silence_detector::VOICE_RMS_THRESHOLD
        {
            self.post_roll_speech_detected.store(true, Ordering::SeqCst);
        }
        if !samples.is_empty() {
            self.sample_count
                .fetch_add(samples.len() as u64, Ordering::Relaxed);
            atomic_add_f64(&self.sum_squares_bits, aggregate_sum_squares);
            atomic_max_f32(&self.peak_bits, peak);
            atomic_max_f32(&self.max_window_rms_bits, window_rms);
            self.evidence_window_sum_squares_bits
                .store(evidence_sum_squares.to_bits(), Ordering::Relaxed);
            self.evidence_window_sample_count
                .store(evidence_sample_count, Ordering::Relaxed);
            self.samples_above_rms_floor
                .fetch_add(samples_above_floor, Ordering::Relaxed);
            self.windows_above_rms_floor
                .fetch_add(windows_above_floor, Ordering::Relaxed);
        }
        window_rms
    }

    fn snapshot(
        &self,
        sample_rate: u32,
        channels: u16,
        speech_detected: bool,
    ) -> CaptureAudioMetrics {
        let sample_count = self.sample_count.load(Ordering::Relaxed);
        let sum_squares = f64::from_bits(self.sum_squares_bits.load(Ordering::Relaxed));
        let frames = sample_count / u64::from(channels.max(1));
        let duration_ms = frames
            .saturating_mul(1000)
            .checked_div(u64::from(sample_rate.max(1)))
            .unwrap_or(0);
        let rms = if sample_count == 0 {
            0.0
        } else {
            (sum_squares / sample_count as f64).sqrt()
        };

        let mut samples_above = self.samples_above_rms_floor.load(Ordering::Relaxed);
        let mut windows_above = self.windows_above_rms_floor.load(Ordering::Relaxed);
        let partial_sample_count = self.evidence_window_sample_count.load(Ordering::Relaxed);
        if partial_sample_count > 0 {
            let partial_sum_squares = f64::from_bits(
                self.evidence_window_sum_squares_bits
                    .load(Ordering::Relaxed),
            );
            let partial_rms = (partial_sum_squares / partial_sample_count as f64).sqrt() as f32;
            if partial_rms > crate::audio::speech_evidence::NO_SPEECH_WINDOW_RMS_FLOOR {
                samples_above = samples_above.saturating_add(partial_sample_count);
                windows_above = windows_above.saturating_add(1);
            }
        }
        let above_frames = samples_above / u64::from(channels.max(1));
        let rate = u64::from(sample_rate.max(1));
        let ms_above_rms_floor = above_frames
            .saturating_mul(1000)
            .saturating_add(rate.saturating_sub(1))
            .checked_div(rate)
            .unwrap_or(0);

        CaptureAudioMetrics {
            sample_count,
            start_to_first_audio_ms: match self.start_to_first_audio_ms.load(Ordering::Relaxed) {
                u64::MAX => None,
                ms => Some(ms),
            },
            duration_ms,
            rms,
            peak: f32::from_bits(self.peak_bits.load(Ordering::Relaxed)),
            max_window_rms: f32::from_bits(self.max_window_rms_bits.load(Ordering::Relaxed)),
            ms_above_rms_floor,
            windows_above_rms_floor: windows_above,
            sample_rate,
            channels,
            speech_detected,
            post_roll_ms: self.post_roll_ms.load(Ordering::SeqCst),
            post_roll_interrupted: self.post_roll_interrupted.load(Ordering::SeqCst),
            post_roll_speech_detected: self.post_roll_speech_detected.load(Ordering::SeqCst),
        }
    }
}

#[cfg(test)]
impl Default for CaptureMetricsAccumulator {
    fn default() -> Self {
        Self::new(100, 1)
    }
}

fn atomic_add_f64(target: &AtomicU64, value: f64) {
    let mut current = target.load(Ordering::Relaxed);
    loop {
        let next = (f64::from_bits(current) + value).to_bits();
        match target.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

fn atomic_max_f32(target: &AtomicU32, value: f32) {
    let mut current = target.load(Ordering::Relaxed);
    while value > f32::from_bits(current) {
        match target.compare_exchange_weak(
            current,
            value.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

fn finish_capture(
    last_capture_metrics: &Mutex<Option<CaptureAudioMetrics>>,
    metrics: CaptureAudioMetrics,
    writer_result: Result<(), String>,
    device_error: Option<String>,
) -> Result<(), String> {
    if let Ok(mut guard) = last_capture_metrics.lock() {
        *guard = Some(metrics);
    }
    writer_result?;
    if let Some(error) = device_error {
        return Err(error);
    }
    Ok(())
}

pub struct AudioRecorder {
    recording_handle: Arc<Mutex<Option<RecordingHandle>>>,
    audio_level_receiver: Arc<Mutex<Option<mpsc::Receiver<f64>>>>,
    silence_event_receiver: Arc<Mutex<Option<mpsc::Receiver<SilenceDetectorEvent>>>>,
    last_capture_metrics: Arc<Mutex<Option<CaptureAudioMetrics>>>,
}

#[derive(Debug)]
pub enum RecordingReadiness {
    Ready { first_audio_ms: u64 },
    Failed(String),
}

struct ReadinessReporter {
    tx: Option<oneshot::Sender<RecordingReadiness>>,
}

impl Drop for ReadinessReporter {
    fn drop(&mut self) {
        report_start_failure(&mut self.tx, "Microphone initialization failed");
    }
}

fn report_first_callback(
    callback_seen: &AtomicBool,
    ready_tx: &mut Option<oneshot::Sender<RecordingReadiness>>,
    started_at: Instant,
) -> Option<u64> {
    if !callback_seen.load(Ordering::SeqCst) {
        return None;
    }
    let first_audio_ms = started_at.elapsed().as_millis() as u64;
    if let Some(tx) = ready_tx.take() {
        let _ = tx.send(RecordingReadiness::Ready { first_audio_ms });
    }
    Some(first_audio_ms)
}

fn report_start_failure(ready_tx: &mut Option<oneshot::Sender<RecordingReadiness>>, error: &str) {
    if let Some(tx) = ready_tx.take() {
        let _ = tx.send(RecordingReadiness::Failed(error.to_string()));
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        // Ensure cleanup on drop
        if let Ok(mut handle_guard) = self.recording_handle.lock() {
            if let Some(handle) = handle_guard.take() {
                // Send stop signal
                if let Err(e) = handle.stop_tx.send(RecorderCommand::Stop {
                    post_roll: Duration::ZERO,
                }) {
                    log::warn!("Failed to send stop signal during drop: {:?}", e);
                }
                // Don't wait for thread in Drop - let it clean up in background
            }
        } else {
            log::error!("Failed to acquire recording handle lock during drop");
        }

        // Clear audio level receiver

        // Clear silence event receiver
        if let Ok(mut receiver_guard) = self.silence_event_receiver.lock() {
            receiver_guard.take();
        } else {
            log::error!("Failed to acquire silence event receiver lock during drop");
        }
        if let Ok(mut receiver_guard) = self.audio_level_receiver.lock() {
            receiver_guard.take();
        } else {
            log::error!("Failed to acquire audio level receiver lock during drop");
        }
    }
}

struct RecordingHandle {
    stop_tx: mpsc::Sender<RecorderCommand>,
    thread_handle: thread::JoinHandle<Result<String, String>>,
}

#[derive(Debug)]
enum RecorderCommand {
    Stop { post_roll: Duration },
}

fn stop_after_post_roll(
    command: Option<&RecorderCommand>,
    stop_rx: &mpsc::Receiver<RecorderCommand>,
    capture_metrics: &CaptureMetricsAccumulator,
    stop_requested: &AtomicBool,
) {
    let post_roll = match command {
        Some(RecorderCommand::Stop { post_roll }) => *post_roll,
        None => Duration::ZERO,
    };
    capture_metrics
        .post_roll_ms
        .store(post_roll.as_millis() as u64, Ordering::SeqCst);
    if !post_roll.is_zero() {
        capture_metrics
            .post_roll_active
            .store(true, Ordering::SeqCst);
        match stop_rx.recv_timeout(post_roll) {
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Ok(RecorderCommand::Stop { .. }) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                capture_metrics
                    .post_roll_interrupted
                    .store(true, Ordering::SeqCst);
            }
        }
        capture_metrics
            .post_roll_active
            .store(false, Ordering::SeqCst);
    }
    stop_requested.store(true, Ordering::SeqCst);
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            recording_handle: Arc::new(Mutex::new(None)),
            audio_level_receiver: Arc::new(Mutex::new(None)),
            silence_event_receiver: Arc::new(Mutex::new(None)),
            last_capture_metrics: Arc::new(Mutex::new(None)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_recording(
        &mut self,
        output_path: &str,
        device_name: Option<String>,
        streaming_tap_enabled: bool,
        recording_generation: u64,
        stream_cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
        stream_sink_factory: Option<StreamTapSinkFactory>,
        started_at: Instant,
    ) -> Result<oneshot::Receiver<RecordingReadiness>, String> {
        log::info!(
            "AudioRecorder::start_recording called with path: {}",
            output_path
        );

        // Acquire lock once and hold it through the entire initialization
        let mut handle_guard = self
            .recording_handle
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?;

        // Check if already recording
        if handle_guard.is_some() {
            return Err("Already recording".to_string());
        }

        // Clear any leftover side-channel receivers from previous recordings
        if let Ok(mut guard) = self.audio_level_receiver.lock() {
            guard.take();
        }
        if let Ok(mut guard) = self.silence_event_receiver.lock() {
            guard.take();
        }
        *self
            .last_capture_metrics
            .lock()
            .map_err(|e| format!("Failed to acquire capture metrics lock: {e}"))? = None;

        let output_path = PathBuf::from(output_path);
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let stop_tx_clone = stop_tx.clone();
        let last_capture_metrics = self.last_capture_metrics.clone();

        // Create audio level channel (f64 for EBU R128 loudness values)
        let (audio_level_tx, audio_level_rx) = mpsc::sync_channel::<f64>(8);
        let (silence_event_tx, silence_event_rx) = mpsc::sync_channel::<SilenceDetectorEvent>(8);
        // Spawn recording thread
        let thread_handle = thread::spawn(move || -> Result<String, String> {
            let mut readiness_reporter = ReadinessReporter { tx: Some(ready_tx) };
            let host = cpal::default_host();
            let device = if let Some(device_name) = device_name {
                // Try to find the specified device
                find_input_device(&host, &device_name)
                    .map_err(|e| format!("Failed to enumerate input devices: {}", e))?
                    .ok_or_else(|| {
                        log::warn!(
                            "Specified device '{}' not found, falling back to default",
                            device_name
                        );
                        format!("Device '{}' not found", device_name)
                    })
                    .or_else(|_| {
                        // Fallback to default device if specified device not found
                        host.default_input_device()
                            .ok_or("No input device available".to_string())
                    })?
            } else {
                // Use default device
                host.default_input_device()
                    .ok_or("No input device available")?
            };

            let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
            log::info!("======================================");
            log::info!("🎤 AUDIO DEVICE SELECTED: {}", device_name);
            log::info!("======================================");

            let config = device.default_input_config().map_err(|e| e.to_string())?;
            log::info!(
                "Audio config: sample_rate={} Hz, channels={}, format={:?}",
                config.sample_rate().0,
                config.channels(),
                config.sample_format()
            );

            // Derive the largest callback payload (in i16 samples) this stream can
            // ever deliver, so every preallocated chunk and scratch conversion
            // buffer is sized to hold it without growing on the real-time thread.
            let chunk_capacity = max_callback_samples(&device, &config);

            // Initialize silence detector and level meter
            let silence_detector = Arc::new(Mutex::new(SilenceDetector::new()));
            let capture_metrics = Arc::new(CaptureMetricsAccumulator::new(
                config.sample_rate().0,
                config.channels(),
            ));
            let level_meter = Arc::new(Mutex::new(
                AudioLevelMeter::new(
                    config.sample_rate().0,
                    config.channels() as u32,
                    audio_level_tx.clone(),
                )
                .map_err(|e| format!("Failed to create level meter: {}", e))?,
            ));

            // Record with native settings, Whisper will handle resampling
            let spec = hound::WavSpec {
                channels: config.channels(),
                sample_rate: config.sample_rate().0,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };

            let writer = hound::WavWriter::create(&output_path, spec).map_err(|e| e.to_string())?;

            let (writer_tx, writer_rx) = mpsc::sync_channel::<WriterMsg>(WRITER_QUEUE_CAPACITY);
            // Bounded so returning a buffer is alloc-free (fixed ring buffer; no
            // per-send heap node like an unbounded channel) and, by conservation
            // (see RECYCLE_CHANNEL_CAPACITY), never blocks the real-time thread.
            let (recycle_tx, recycle_rx) = mpsc::sync_channel::<Vec<i16>>(RECYCLE_CHANNEL_CAPACITY);
            let recycle_tx_for_drop = recycle_tx.clone();
            // Preallocate the chunk pool BEFORE the stream starts so the
            // real-time audio callback reuses existing buffers and never
            // allocates on the audio thread (plan 008: lock-free, no-alloc path).
            for _ in 0..(WRITER_QUEUE_CAPACITY + CHUNK_POOL_SLACK) {
                let _ = recycle_tx.send(Vec::with_capacity(chunk_capacity));
            }
            let bytes_written = Arc::new(AtomicU64::new(0));
            let dropped_chunks = Arc::new(AtomicU64::new(0));
            let writer_bytes = bytes_written.clone();
            let writer_dropped = dropped_chunks.clone();
            let stop_tx_for_size = stop_tx_clone.clone();
            let (play_tx, play_rx) = mpsc::channel();
            let (stream_tap_rt, stream_tap_finalizer) = if let Some(handle) =
                stream_tap::maybe_spawn_noop_worker_after_play(
                    streaming_tap_enabled,
                    recording_generation,
                    chunk_capacity,
                    stream_cancelled,
                    stream_sink_factory,
                    config.sample_rate().0,
                    config.channels(),
                    play_rx,
                ) {
                let (rt, finalizer) = handle.into_rt();
                (Some(rt), Some(finalizer))
            } else {
                (None, None)
            };
            // Disconnect-independent finalize signal. The RT callback holds a
            // `writer_tx` clone that can outlive stop on the Windows
            // stream-drop-timeout path, so neither a `Full` `Finalize` send nor
            // a channel disconnect is guaranteed. This flag makes the writer
            // worker always exit and finalize the WAV.
            let finalize_flag = Arc::new(AtomicBool::new(false));
            let writer_finalize_flag = finalize_flag.clone();
            let writer_handle = thread::spawn(move || -> Result<(), String> {
                let mut writer = writer;
                let mut write_error = None::<String>;

                loop {
                    let mut samples = match next_writer_action(
                        writer_rx.recv_timeout(Duration::from_millis(100)),
                        writer_finalize_flag.load(Ordering::SeqCst),
                    ) {
                        WriterAction::Write(samples) => samples,
                        WriterAction::Stop => break,
                        WriterAction::Idle => continue,
                    };
                    let sample_bytes = (samples.len() * 2) as u64;
                    // Block write: hound's monomorphized i16 sample writer
                    // buffers the whole chunk in a recycled internal buffer and
                    // flushes it with a single `write_all`, instead of calling
                    // `WavWriter::write_sample` (per-sample format dispatch +
                    // BufWriter interaction) once per sample. This is hound's
                    // documented fast path and keeps the writer from
                    // backlogging under stop — that backlog was the root cause
                    // of the old "failed to stop within timeout" →
                    // unfinalized-WAV → deleted-recording chain.
                    // `SampleWriter16::flush` panics only if fewer than
                    // `samples.len()` samples were written; we write exactly
                    // that many, so it cannot panic here.
                    {
                        let mut sample_writer = writer.get_i16_writer(samples.len() as u32);
                        for &sample in &samples {
                            sample_writer.write_sample(sample);
                        }
                        if let Err(e) = sample_writer.flush() {
                            write_error = Some(format!("Failed to write audio samples: {}", e));
                        }
                    }

                    if write_error.is_some() {
                        break;
                    }

                    let new_total =
                        writer_bytes.fetch_add(sample_bytes, Ordering::SeqCst) + sample_bytes;
                    if RecordingSize::check(new_total).is_err() {
                        let _ = stop_tx_for_size.send(RecorderCommand::Stop {
                            post_roll: Duration::ZERO,
                        });
                    }

                    samples.clear();
                    let _ = recycle_tx.send(samples);
                }

                writer
                    .finalize()
                    .map_err(|e| format!("WAV finalize failed: {e}"))?;

                let dropped = writer_dropped.load(Ordering::SeqCst);
                if dropped > 0 {
                    log::warn!(
                        "Dropped {} audio chunks because the writer queue was full",
                        dropped
                    );
                    return Err(format!(
                        "Recording integrity failure: dropped {dropped} audio chunks before WAV finalization"
                    ));
                }

                if let Some(error) = write_error {
                    return Err(error);
                }

                Ok(())
            });

            // Error callback that triggers stop on device errors (e.g., disconnection)
            let stop_tx_for_error = stop_tx_clone.clone();
            let error_occurred = Arc::new(Mutex::new(None::<String>));
            let error_occurred_for_callback = error_occurred.clone();

            let err_fn = move |err: cpal::StreamError| {
                // Detailed logging for audio device errors
                log::error!("═══════════════════════════════════════════════════════");
                log::error!("🔴 AUDIO DEVICE ERROR DETECTED");
                log::error!("═══════════════════════════════════════════════════════");
                log::error!("Error type: {:?}", err);
                log::error!("Error message: {}", err);
                log::error!("Action: Triggering graceful recording stop");
                log::error!("═══════════════════════════════════════════════════════");

                // Store the error
                if let Ok(mut guard) = error_occurred_for_callback.lock() {
                    *guard = Some(format!("Audio device error: {}", err));
                }
                // Signal the recording thread to stop
                let _ = stop_tx_for_error.send(RecorderCommand::Stop {
                    post_roll: Duration::ZERO,
                });
            };

            // Drain barrier flags shared between callback and stop path
            let stop_requested = Arc::new(AtomicBool::new(false));
            let callback_drained = Arc::new(AtomicBool::new(false));
            let first_callback = Arc::new(AtomicBool::new(false));

            // Common audio processing closure
            let process_audio = {
                let writer_tx_clone: SyncSender<WriterMsg> = writer_tx.clone();
                let dropped_chunks_clone = dropped_chunks.clone();
                let recycle_tx_for_drop = recycle_tx_for_drop.clone();
                let silence_event_tx_clone = silence_event_tx.clone();
                let silence_detector_clone = silence_detector.clone();
                let level_meter_clone = level_meter.clone();
                let stop_requested_clone = stop_requested.clone();
                let callback_drained_clone = callback_drained.clone();
                let stream_tap_rt: Option<StreamTapRt> = stream_tap_rt;
                let capture_metrics_clone = capture_metrics.clone();
                let first_callback_clone = first_callback.clone();

                move |f32_samples: &[f32], i16_samples: &[i16]| {
                    first_callback_clone.store(true, Ordering::SeqCst);
                    // A panic in this real-time path would unwind into CPAL's
                    // `extern "C"` WASAPI callback (CPAL wraps it in no catch_unwind)
                    // and ABORT the whole process. Catch it and drop this buffer
                    // instead of crashing — covers both the recording and the
                    // stop/drain-window callbacks (the silent Windows stop crash).
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // Drain barrier: if stop was requested, handle final write then exit
                        if stop_requested_clone.load(Ordering::SeqCst) {
                            // Only write on the first callback after stop; skip all subsequent ones
                            if !callback_drained_clone.load(Ordering::SeqCst) {
                                // Plan 060.1: the final buffer contributes capture
                                // speech evidence (no second scan, no allocation) and
                                // an unwritable final buffer is counted as dropped so
                                // the integrity check — not the no-speech gate — owns
                                // the outcome.
                                drain_final_callback(
                                    f32_samples,
                                    i16_samples,
                                    &capture_metrics_clone,
                                    &recycle_rx,
                                    &writer_tx_clone,
                                    &recycle_tx_for_drop,
                                    &dropped_chunks_clone,
                                    stream_tap_rt.as_ref(),
                                );
                                callback_drained_clone.store(true, Ordering::SeqCst);
                            }
                            return;
                        }
                        // Reuse one traversal for callback RMS and recording-wide metrics.
                        let rms = capture_metrics_clone.observe(f32_samples);

                        // Process with level meter
                        if let Ok(mut meter) = level_meter_clone.try_lock() {
                            let _ = meter.process_samples(f32_samples);
                        }

                        // Emit sparse silence detector events over a bounded side channel.
                        if let Ok(mut detector) = silence_detector_clone.try_lock() {
                            if let Some(event) = detector.update(rms) {
                                let _ = silence_event_tx_clone.try_send(event);
                            }
                        }

                        let Ok(mut chunk) = recycle_rx.try_recv() else {
                            // Pool exhausted (all buffers in flight under extreme
                            // backpressure): drop this chunk rather than allocate on
                            // the RT thread (consistent with the queue-full drop path).
                            dropped_chunks_clone.fetch_add(1, Ordering::SeqCst);
                            return;
                        };
                        chunk.clear();
                        chunk.extend_from_slice(i16_samples);

                        match writer_tx_clone.try_send(WriterMsg::Chunk(chunk)) {
                            Ok(()) => {}
                            Err(TrySendError::Full(WriterMsg::Chunk(mut chunk))) => {
                                dropped_chunks_clone.fetch_add(1, Ordering::SeqCst);
                                chunk.clear();
                                // try_send: never blocks or allocates on the RT
                                // thread. By conservation (RECYCLE_CHANNEL_CAPACITY)
                                // the channel always has room; on the impossible
                                // full case the chunk is simply dropped.
                                let _ = recycle_tx_for_drop.try_send(chunk);
                            }
                            Err(TrySendError::Full(WriterMsg::Finalize)) => {}
                            Err(TrySendError::Disconnected(_)) => {}
                        }
                        if let Some(tap) = stream_tap_rt.as_ref() {
                            stream_tap::enqueue_frame_rt(tap, i16_samples);
                        }
                    }));
                }
            };

            let build_start = Instant::now();
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => {
                    let process_audio = process_audio;
                    let mut i16_scratch: Vec<i16> = Vec::with_capacity(chunk_capacity);
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[f32], _: &_| {
                                // Convert F32 to I16 with proper clamping to avoid distortion
                                i16_scratch.clear();
                                i16_scratch.extend(data.iter().map(|&sample| f32_to_i16(sample)));

                                // Process audio
                                process_audio(data, &i16_scratch);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::I16 => {
                    let process_audio = process_audio;
                    let mut f32_scratch: Vec<f32> = Vec::with_capacity(chunk_capacity);
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[i16], _: &_| {
                                // Convert I16 to F32 for processing
                                f32_scratch.clear();
                                f32_scratch.extend(data.iter().map(|&sample| i16_to_f32(sample)));

                                // Process audio
                                process_audio(&f32_scratch, data);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                cpal::SampleFormat::U16 => {
                    let process_audio = process_audio;
                    let mut f32_scratch: Vec<f32> = Vec::with_capacity(chunk_capacity);
                    let mut i16_scratch: Vec<i16> = Vec::with_capacity(chunk_capacity);
                    device
                        .build_input_stream(
                            &config.config(),
                            move |data: &[u16], _: &_| {
                                // Convert U16 to F32 for processing
                                f32_scratch.clear();
                                f32_scratch.extend(data.iter().map(|&sample| u16_to_f32(sample)));

                                // Convert U16 to I16 for writing
                                i16_scratch.clear();
                                i16_scratch.extend(data.iter().map(|&sample| u16_to_i16(sample)));

                                // Process audio
                                process_audio(&f32_scratch, &i16_scratch);
                            },
                            err_fn,
                            None,
                        )
                        .map_err(|e| e.to_string())?
                }
                _ => {
                    return Err(format!(
                        "Unsupported sample format: {:?}",
                        config.sample_format()
                    ))
                }
            };
            log::debug!(
                "⏱️ [REC TIMING] build_input_stream={}ms (+{}ms)",
                build_start.elapsed().as_millis(),
                started_at.elapsed().as_millis()
            );

            let play_start = Instant::now();
            stream.play().map_err(|e| {
                log::error!("Failed to start audio stream: {}", e);
                e.to_string()
            })?;
            log::debug!(
                "⏱️ [REC TIMING] stream.play={}ms (+{}ms)",
                play_start.elapsed().as_millis(),
                started_at.elapsed().as_millis()
            );
            // The tap worker creates its sink only after play. Frames captured
            // while a cold preview sink initializes still reach the WAV writer.
            let _ = play_tx.send(());

            log::info!("Audio stream started successfully");

            // Wait for stop signal
            let stop_reason = loop {
                if let Some(first_audio_ms) =
                    report_first_callback(&first_callback, &mut readiness_reporter.tx, started_at)
                {
                    capture_metrics
                        .start_to_first_audio_ms
                        .store(first_audio_ms, Ordering::Relaxed);
                    log::debug!("⏱️ [REC TIMING] first callback (+{}ms)", first_audio_ms);
                    break stop_rx.recv().ok();
                }
                match stop_rx.recv_timeout(Duration::from_millis(2)) {
                    Ok(reason) => {
                        report_start_failure(
                            &mut readiness_reporter.tx,
                            "Recording stopped before first audio callback",
                        );
                        break Some(reason);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break None,
                }
            };

            // Drain barrier: signal callback to drain and wait for acknowledgment
            stop_after_post_roll(
                stop_reason.as_ref(),
                &stop_rx,
                &capture_metrics,
                &stop_requested,
            );
            let drain_start = Instant::now();
            while !callback_drained.load(Ordering::SeqCst) {
                if drain_start.elapsed() > Duration::from_millis(200) {
                    log::warn!(
                        "Drain timeout: proceeding with finalization after {}ms",
                        drain_start.elapsed().as_millis()
                    );
                    callback_drained.store(true, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            if callback_drained.load(Ordering::SeqCst) {
                log::info!("Drain complete: callback acknowledged stop");
            }

            // Pause the stream to stop audio capture
            if let Err(e) = stream.pause() {
                log::warn!("Failed to pause audio stream: {}", e);
            }

            // Platform-specific stream cleanup.
            // On Windows, some USB/wireless WASAPI devices can hang during
            // Stream::drop(). We attempt a clean drop with a timeout guard;
            // if it hangs beyond 3 seconds, we fall back to mem::forget.
            #[cfg(target_os = "windows")]
            {
                let stream_drop_result = std::sync::mpsc::channel::<()>();
                let (drop_tx, drop_rx) = stream_drop_result;
                // Move stream into a thread so drop doesn't block the recording thread
                std::thread::spawn(move || {
                    drop(stream);
                    let _ = drop_tx.send(());
                });
                match drop_rx.recv_timeout(Duration::from_secs(3)) {
                    Ok(()) => log::info!("Audio stream stopped and cleaned up"),
                    Err(_) => log::warn!(
                        "Audio stream drop timed out (3s). Stream resources will be reclaimed on process exit."
                    ),
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                drop(stream);
                log::info!("Audio stream stopped and cleaned up");
            }

            // Signal the writer to finalize, NON-blockingly. A blocking
            // `send(Finalize)` could stall teardown if the queue were full or
            // the writer were blocked on a slow/hung filesystem. `try_send`
            // cannot block; if the message does not fit, the following
            // `drop(writer_tx)` disconnects the channel and the writer's recv
            // loop exits anyway, so finalize still runs either way. (The stream
            // was already dropped above, so no new Chunks can arrive and this
            // is the last sender.)
            // Signal finalize via the flag BEFORE the best-effort message, so a
            // full queue or a still-alive callback sender (Windows stream-drop
            // timeout) cannot prevent the writer from finalizing.
            finalize_flag.store(true, Ordering::SeqCst);
            let _ = writer_tx.try_send(WriterMsg::Finalize);
            drop(writer_tx);
            let stream_tap_worker = stream_tap_finalizer.map(|finalizer| finalizer.finalize());

            // Bound the writer join so a slow/hung writer cannot block teardown
            // indefinitely. On timeout the worker is detached — hound's
            // `WavWriter::drop` finalizes the header when it eventually exits —
            // so a normally-terminating worker never leaves the file
            // unfinalized-but-alive. See [`WRITER_JOIN_TIMEOUT`] and
            // [`join_writer_bounded`].
            let writer_result = join_writer_bounded(writer_handle, WRITER_JOIN_TIMEOUT);
            if let Some(worker) = stream_tap_worker {
                let _ = join_stream_tap_bounded(worker, STREAM_TAP_JOIN_TIMEOUT);
            }
            let speech_detected = silence_detector
                .lock()
                .map(|detector| detector.speech_detected())
                .unwrap_or(false);
            let metrics = capture_metrics.snapshot(
                config.sample_rate().0,
                config.channels(),
                speech_detected,
            );
            let device_error = error_occurred.lock().ok().and_then(|guard| guard.clone());
            finish_capture(&last_capture_metrics, metrics, writer_result, device_error)?;

            // Return appropriate message based on stop reason
            match stop_reason {
                Some(RecorderCommand::Stop { .. }) => Ok("Recording stopped by user".to_string()),
                None => Ok("Recording stopped".to_string()),
            }
        });

        // Set the handle using the guard we already have
        *handle_guard = Some(RecordingHandle {
            stop_tx,
            thread_handle,
        });

        // Store the audio level receiver

        // Store the silence event receiver
        *self
            .silence_event_receiver
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))? = Some(silence_event_rx);
        *self
            .audio_level_receiver
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))? = Some(audio_level_rx);

        Ok(ready_rx)
    }

    pub fn stop_recording(&mut self) -> Result<String, String> {
        self.stop_recording_with_post_roll(Duration::ZERO)
    }

    pub fn stop_recording_with_post_roll(&mut self, post_roll: Duration) -> Result<String, String> {
        let handle = self
            .recording_handle
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?
            .take();

        // Also clear the audio level receiver

        // Also clear the silence event receiver
        if let Ok(mut guard) = self.silence_event_receiver.lock() {
            guard.take();
        }
        if let Ok(mut guard) = self.audio_level_receiver.lock() {
            guard.take();
        }

        if let Some(handle) = handle {
            // Send stop signal
            handle
                .stop_tx
                .send(RecorderCommand::Stop { post_roll })
                .ok();

            // Wait for the recording thread to finish, bounded by a SINGLE
            // teardown deadline that covers every internal sub-budget (drain
            // post-roll + window + platform stream drop + writer finalize). See
            // [`STOP_JOIN_TIMEOUT`]. Replaces the old independent 5s poll that
            // raced the previously-unbounded writer join.
            let thread_handle = handle.thread_handle;
            let timeout = STOP_JOIN_TIMEOUT;
            let start = std::time::Instant::now();

            // Try to join thread with timeout by checking if it's finished
            while start.elapsed() < timeout {
                if thread_handle.is_finished() {
                    match thread_handle.join() {
                        Ok(Ok(msg)) => return Ok(msg),
                        Ok(Err(e)) => return Err(e),
                        Err(_) => return Err("Recording thread panicked".to_string()),
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }

            // If we get here, the thread didn't finish within timeout
            Err("Recording thread failed to stop within timeout".to_string())
        } else {
            Err("Not recording".to_string())
        }
    }

    pub fn take_last_capture_metrics(&self) -> Option<CaptureAudioMetrics> {
        self.last_capture_metrics
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    pub fn wait_for_recording_end(&mut self) -> Result<String, String> {
        let handle = self
            .recording_handle
            .lock()
            .map_err(|e| format!("Failed to acquire lock: {}", e))?
            .take();

        if let Some(handle) = handle {
            match handle.thread_handle.join() {
                Ok(Ok(msg)) => Ok(msg),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("Recording thread panicked".to_string()),
            }
        } else {
            Err("Not recording".to_string())
        }
    }

    pub fn take_silence_event_receiver(&mut self) -> Option<mpsc::Receiver<SilenceDetectorEvent>> {
        self.silence_event_receiver
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    pub fn is_recording(&self) -> bool {
        self.recording_handle
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    /// Returns true if a recording worker exists but its thread has already
    /// finished. Non-consuming: the join handle stays stored so the normal
    /// stop path can still take and join it.
    pub fn recording_thread_finished(&self) -> bool {
        self.recording_handle
            .lock()
            .map(|guard| {
                guard
                    .as_ref()
                    .map(|handle| handle.thread_handle.is_finished())
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    pub fn take_audio_level_receiver(&mut self) -> Option<mpsc::Receiver<f64>> {
        self.audio_level_receiver
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    pub fn get_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| devices.filter_map(|device| device.name().ok()).collect())
            .unwrap_or_else(|_| Vec::new())
    }
}

/// Joins the WAV writer worker with a hard deadline.
///
/// `std::thread::JoinHandle` exposes no native timed join, so we poll
/// `is_finished()` at a short cadence. With the block-write fast path the
/// writer drains its bounded queue in microseconds, so this completes on the
/// first check in practice; the deadline is pure defense against a writer
/// stalled on a pathological filesystem.
///
/// On timeout the handle is dropped (the worker is detached) and we return an
/// error that [`stop_error_is_unfinalized`] classifies as "do not transcribe".
/// hound's `WavWriter::drop` updates the WAV header when the detached worker
/// eventually exits, so the file is never left unfinalized-but-alive by a
/// normally-terminating worker — only a worker that is *hung forever* (e.g. an
/// unresponsive network filesystem) can leave the header stale, and in that
/// case the command layer refuses to transcribe AND refuses to delete the file
/// (see the `stop_unfinalized` branch in the `stop_recording` command), so no
/// half-written file is ever fed to the model or torn out from under the worker.
fn join_writer_bounded(
    handle: thread::JoinHandle<Result<(), String>>,
    deadline: Duration,
) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if handle.is_finished() {
            return match handle.join() {
                Ok(result) => result,
                Err(_) => Err("Writer thread panicked".to_string()),
            };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    log::error!(
        "Writer did not finalize within {}ms; detaching worker (hound Drop will finalize on exit)",
        deadline.as_millis()
    );
    Err("Writer thread failed to finalize within timeout".to_string())
}

fn join_stream_tap_bounded(
    handle: thread::JoinHandle<StreamTapWorkerSummary>,
    deadline: Duration,
) -> Option<StreamTapWorkerSummary> {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if handle.is_finished() {
            return match handle.join() {
                Ok(summary) => Some(summary),
                Err(_) => {
                    log::warn!("Stream tap worker panicked after WAV finalization");
                    None
                }
            };
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    log::warn!(
        "Stream tap worker did not finish within {}ms; detaching so preview finalization cannot block the WAV stop path",
        deadline.as_millis()
    );
    None
}

/// Classifies an error returned by [`AudioRecorder::stop_recording`]. Returns true when the
/// recording worker did NOT finish, or the WAV writer could not finalize the file, so the
/// capture must not be transcribed. A worker that finishes with an error (e.g. a CPAL device
/// error) finalizes the WAV first, so those return false and the captured audio can still be
/// recovered.
pub(crate) fn stop_error_is_unfinalized(error: &str) -> bool {
    error.contains("failed to stop within timeout")
        || error.contains("failed to finalize within timeout")
        || error.contains("panicked")
        || error.contains("WAV finalize failed:")
}

pub(crate) fn stop_error_is_integrity_failure(error: &str) -> bool {
    error.starts_with("Recording integrity failure:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_waits_for_first_callback_flag() {
        let flag = AtomicBool::new(false);
        let (tx, rx) = oneshot::channel();
        let mut tx = Some(tx);
        let started_at = Instant::now();
        assert_eq!(report_first_callback(&flag, &mut tx, started_at), None);
        assert!(tx.is_some());
        flag.store(true, Ordering::SeqCst);
        let elapsed = report_first_callback(&flag, &mut tx, started_at).unwrap();
        assert!(tx.is_none());
        assert!(
            matches!(rx.blocking_recv(), Ok(RecordingReadiness::Ready { first_audio_ms }) if first_audio_ms == elapsed)
        );
    }

    #[test]
    fn forced_start_error_reports_failed_once() {
        let (tx, rx) = oneshot::channel();
        let mut reporter = ReadinessReporter { tx: Some(tx) };
        report_start_failure(&mut reporter.tx, "device failed");
        drop(reporter);
        assert!(
            matches!(rx.blocking_recv(), Ok(RecordingReadiness::Failed(error)) if error == "device failed")
        );
    }

    #[test]
    fn early_worker_exit_reports_failed() {
        let (tx, rx) = oneshot::channel();
        drop(ReadinessReporter { tx: Some(tx) });
        assert!(
            matches!(rx.blocking_recv(), Ok(RecordingReadiness::Failed(error)) if error == "Microphone initialization failed")
        );
    }

    #[test]
    fn next_writer_action_writes_chunk() {
        assert!(matches!(
            next_writer_action(Ok(WriterMsg::Chunk(vec![1, 2, 3])), false),
            WriterAction::Write(s) if s == vec![1, 2, 3]
        ));
    }

    #[test]
    fn next_writer_action_stops_on_finalize_message() {
        assert!(matches!(
            next_writer_action(Ok(WriterMsg::Finalize), false),
            WriterAction::Stop
        ));
    }

    #[test]
    fn next_writer_action_stops_on_disconnect() {
        assert!(matches!(
            next_writer_action(Err(std::sync::mpsc::RecvTimeoutError::Disconnected), false),
            WriterAction::Stop
        ));
    }

    #[test]
    fn next_writer_action_idles_on_timeout_without_flag() {
        assert!(matches!(
            next_writer_action(Err(std::sync::mpsc::RecvTimeoutError::Timeout), false),
            WriterAction::Idle
        ));
    }

    #[test]
    fn next_writer_action_finalizes_on_timeout_with_flag() {
        // P1 fix: even when the channel never disconnects (a callback sender
        // clone is still alive) and no Finalize message arrives, the finalize
        // flag makes the writer stop and finalize the WAV.
        assert!(matches!(
            next_writer_action(Err(std::sync::mpsc::RecvTimeoutError::Timeout), true),
            WriterAction::Stop
        ));
    }

    #[test]
    fn stop_error_is_unfinalized_distinguishes_finalized_from_unfinalized() {
        // Worker did not finish -> WAV not finalized -> must not transcribe.
        assert!(stop_error_is_unfinalized(
            "Recording thread failed to stop within timeout"
        ));
        assert!(stop_error_is_unfinalized("Recording thread panicked"));
        assert!(stop_error_is_unfinalized("Writer thread panicked"));
        assert!(stop_error_is_unfinalized(
            "WAV finalize failed: failed to seek"
        ));
        assert!(stop_error_is_unfinalized(
            "Writer thread failed to finalize within timeout"
        ));
        // Worker finished with an error -> WAV finalized -> recoverable, transcribe.
        assert!(!stop_error_is_unfinalized(
            "Audio device error: device disconnected"
        ));
    }

    #[test]
    fn stop_error_is_integrity_failure_matches_dropped_chunk_marker() {
        assert!(stop_error_is_integrity_failure(
            "Recording integrity failure: dropped 2 audio chunks before WAV finalization"
        ));
        assert!(!stop_error_is_integrity_failure(
            "WAV finalize failed: failed to seek"
        ));
    }

    #[test]
    fn stream_tap_join_timeout_detaches_slow_worker() {
        let handle = thread::spawn(|| {
            thread::sleep(Duration::from_millis(80));
            StreamTapWorkerSummary {
                generation: 1,
                frames: 0,
                samples: 0,
                dropped: 0,
                cancelled: false,
                stale: false,
            }
        });

        let started = Instant::now();
        let summary = join_stream_tap_bounded(handle, Duration::from_millis(10));

        assert!(summary.is_none());
        assert!(started.elapsed() < Duration::from_millis(60));
    }

    #[test]
    fn test_recording_size_check() {
        // Under 500MB should be accepted
        let under_limit: u64 = 499 * 1024 * 1024;
        assert!(RecordingSize::check(under_limit).is_ok());

        // Exactly 500MB should be accepted
        let exactly_limit: u64 = 500 * 1024 * 1024;
        assert!(RecordingSize::check(exactly_limit).is_ok());

        // Over 500MB should be rejected
        let over_limit: u64 = 500 * 1024 * 1024 + 1;
        assert!(RecordingSize::check(over_limit).is_err());

        // Zero bytes is fine
        assert!(RecordingSize::check(0).is_ok());
    }
    #[test]
    fn chunk_pool_depth_covers_max_inflight_buffers() {
        // Invariant for the no-alloc RT audio callback (plan 008): the
        // preallocated pool must hold more buffers than can ever be in flight at
        // once. At most WRITER_QUEUE_CAPACITY sit in the bounded writer queue, one
        // is mid-write in the writer thread, and one is being assembled in the
        // callback — so the slack must cover those two extra slots. By conservation
        // this guarantees the recycle channel is never empty during capture, so the
        // callback only ever reuses a buffer and never allocates on the audio thread.
        const _: () = assert!(
            CHUNK_POOL_SLACK >= 2,
            "pool slack must cover the writer-in-progress + callback-in-progress buffers"
        );
    }

    #[test]
    fn chunk_capacity_covers_any_realistic_callback_without_growing() {
        // Reproduces the real-time allocation bug: a recycled chunk preallocated
        // to a FIXED capacity smaller than a real callback forces
        // `extend_from_slice` to reallocate on the audio thread. The pre-fix code
        // used a hard-coded 4096-sample capacity, which a 4096-frame * 2-channel
        // callback (8192 samples) already exceeds. The capacity must always be
        // >= the full callback payload (max_frames * channels).
        for &(frames, channels) in &[
            (512usize, 2usize),
            (1024, 2),
            (2048, 2),
            (4096, 2),
            (8192, 1),
            (8192, 2),
        ] {
            let payload = frames * channels;
            let cap = chunk_capacity_for(frames, channels);
            assert!(
                cap >= payload,
                "frames={frames} channels={channels}: chunk capacity {cap} < callback payload \
                 {payload}; the RT thread would reallocate on extend_from_slice"
            );
        }
    }

    #[test]
    fn chunk_capacity_floored_above_underreported_ranges() {
        // A device that under-reports its buffer range must still leave headroom
        // so a reused buffer never grows. The pre-fix 4096-capacity chunks
        // violated this for any callback larger than 4096 samples.
        assert!(chunk_capacity_for(64, 1) >= CHUNK_CAPACITY_MIN);
        assert!(chunk_capacity_for(64, 1) > 4096);
    }

    #[test]
    fn sane_max_frames_rejects_bogus_wasapi_max() {
        // The v2.0.0 Windows crash: cpal's WASAPI software stack reports
        // `SupportedBufferSize::Range { max: u32::MAX }`. Trusting it made the
        // chunk pool preallocate terabytes and abort the process. The sanitizer
        // must reject u32::MAX (and 0) so callers fall back to a safe default.
        assert_eq!(sane_max_frames(u32::MAX), None);
        assert_eq!(sane_max_frames(0), None);
        assert_eq!(sane_max_frames(480), Some(480));
        assert_eq!(
            sane_max_frames(MAX_REASONABLE_CALLBACK_FRAMES as u32),
            Some(MAX_REASONABLE_CALLBACK_FRAMES)
        );
        assert_eq!(
            sane_max_frames(MAX_REASONABLE_CALLBACK_FRAMES as u32 + 1),
            None
        );

        // Fallback path stays tiny: a rejected/unknown max yields a bounded
        // per-chunk capacity (no giant preallocation).
        assert!(chunk_capacity_for(CHUNK_FALLBACK_FRAMES, 2) <= CHUNK_FALLBACK_FRAMES * 2);
    }

    #[test]
    fn recycle_channel_is_bounded_so_rt_return_is_alloc_free() {
        // Reproduces the recycle-path allocation bug: the queue-full drop branch
        // returns a chunk through the recycle channel ON the real-time thread. An
        // UNBOUNDED mpsc::channel heap-allocates a node per send — an allocation
        // on the RT thread. A bounded sync_channel is backed by a fixed ring
        // buffer (no per-send allocation) and, by conservation, never fills, so
        // the RT try_send always succeeds without allocating or blocking.

        // The conservation invariant: the channel bound must equal the whole
        // preallocated pool depth, so returning a buffer always has room.
        assert_eq!(
            RECYCLE_CHANNEL_CAPACITY,
            WRITER_QUEUE_CAPACITY + CHUNK_POOL_SLACK
        );

        // The entire pool must fit the bounded channel without blocking
        // (mirrors pool population in start_recording). One more must eventually
        // be refused — proving the channel is bounded (fixed ring buffer, hence
        // alloc-free), unlike an unbounded channel which accepts indefinitely.
        let (tx, _rx) = mpsc::sync_channel::<Vec<i16>>(RECYCLE_CHANNEL_CAPACITY);
        for _ in 0..RECYCLE_CHANNEL_CAPACITY {
            assert!(
                tx.try_send(Vec::new()).is_ok(),
                "pool population must fit the bounded recycle channel"
            );
        }
        // Try a few extra sends: a bounded channel must refuse at least one.
        let refused = (0..CHUNK_POOL_SLACK + 1).any(|_| tx.try_send(Vec::new()).is_err());
        assert!(
            refused,
            "sync_channel must be bounded; an unbounded channel would accept all sends"
        );
    }

    #[test]
    fn test_drain_flag_signaling() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;

        let stop_requested = Arc::new(AtomicBool::new(false));
        let callback_drained = Arc::new(AtomicBool::new(false));

        let stop_clone = stop_requested.clone();
        let drained_clone = callback_drained.clone();

        // Spawn a thread simulating the audio callback
        let handle = thread::spawn(move || {
            // Simulate callback loop checking the drain flag
            while !stop_clone.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
            // Acknowledge drain
            drained_clone.store(true, Ordering::SeqCst);
        });

        // Main thread signals stop
        stop_requested.store(true, Ordering::SeqCst);

        // Spin-wait for drain acknowledgment with 200ms timeout
        let drain_start = Instant::now();
        while !callback_drained.load(Ordering::SeqCst) {
            if drain_start.elapsed() > Duration::from_millis(200) {
                panic!("Drain flag was not set within 200ms timeout");
            }
            thread::sleep(Duration::from_millis(1));
        }

        handle.join().expect("Callback thread should not panic");
        assert!(callback_drained.load(Ordering::SeqCst));
    }

    #[test]
    fn test_f32_to_i16_clamps_at_unit_bounds() {
        assert_eq!(f32_to_i16(1.5), 32767);
        assert_eq!(f32_to_i16(1.0), 32767);
        assert_eq!(f32_to_i16(0.0), 0);
        assert_eq!(f32_to_i16(-1.0), -32767);
        assert_eq!(f32_to_i16(-1.5), -32767);
    }

    #[test]
    fn test_u16_to_i16_midpoint_and_symmetry() {
        assert_eq!(u16_to_i16(32768), 0);
        assert_eq!(u16_to_i16(32769), 1);
        assert_eq!(u16_to_i16(32767), -1);
        assert_eq!(u16_to_i16(u16::MAX), i16::MAX);
        assert_eq!(u16_to_i16(0), i16::MIN);
    }

    #[test]
    fn test_u16_to_f32_midpoint_and_symmetry() {
        assert_eq!(u16_to_f32(32768), 0.0);
        assert_eq!(u16_to_f32(0), -1.0);
        assert!((u16_to_f32(u16::MAX) - (32767.0 / 32768.0)).abs() < f32::EPSILON);
        assert_eq!(u16_to_f32(32769), 1.0 / 32768.0);
        assert_eq!(u16_to_f32(32767), -1.0 / 32768.0);
    }

    #[test]
    fn test_wait_for_recording_end_returns_thread_result() {
        let (stop_tx, _stop_rx) = mpsc::channel();
        let thread_handle =
            thread::spawn(|| Ok::<String, String>("Recording stopped by user".to_string()));
        let handle = RecordingHandle {
            stop_tx,
            thread_handle,
        };

        let mut recorder = AudioRecorder::new();
        {
            let mut guard = recorder.recording_handle.lock().unwrap();
            *guard = Some(handle);
        }

        let result = recorder.wait_for_recording_end().unwrap();
        assert_eq!(result, "Recording stopped by user");
        assert!(!recorder.is_recording());
    }

    #[test]
    fn stop_recording_surfaces_worker_error_and_clears_handle() {
        let (stop_tx, _stop_rx) = mpsc::channel::<RecorderCommand>();
        let thread_handle = thread::spawn(|| Err::<String, String>("device failed".to_string()));
        let mut recorder = AudioRecorder::new();

        *recorder.recording_handle.lock().unwrap() = Some(RecordingHandle {
            stop_tx,
            thread_handle,
        });

        let err = recorder.stop_recording().unwrap_err();
        assert_eq!(err, "device failed");
        assert!(!recorder.is_recording());
    }

    #[test]
    fn stop_recording_requests_immediate_drain() {
        let (stop_tx, stop_rx) = mpsc::channel();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let worker_stop_requested = stop_requested.clone();
        let thread_handle = thread::spawn(move || {
            let command = stop_rx.recv().unwrap();
            assert!(matches!(
                &command,
                RecorderCommand::Stop { post_roll } if post_roll.is_zero()
            ));
            let metrics = CaptureMetricsAccumulator::default();
            stop_after_post_roll(Some(&command), &stop_rx, &metrics, &worker_stop_requested);
            assert_eq!(metrics.snapshot(100, 1, false).post_roll_ms, 0);
            Ok::<String, String>("stopped".to_string())
        });
        let mut recorder = AudioRecorder::new();
        *recorder.recording_handle.lock().unwrap() = Some(RecordingHandle {
            stop_tx,
            thread_handle,
        });

        assert_eq!(recorder.stop_recording().unwrap(), "stopped");
        assert!(stop_requested.load(Ordering::SeqCst));
    }

    #[test]
    fn post_roll_delays_drain_barrier() {
        let post_roll = Duration::from_millis(60);
        let (_stop_tx, stop_rx) = mpsc::channel();
        let command = RecorderCommand::Stop { post_roll };
        let metrics = CaptureMetricsAccumulator::default();
        let stop_requested = AtomicBool::new(false);

        assert!(!stop_requested.load(Ordering::SeqCst));
        let start = Instant::now();
        stop_after_post_roll(Some(&command), &stop_rx, &metrics, &stop_requested);
        assert!(start.elapsed() >= post_roll);
        assert!(stop_requested.load(Ordering::SeqCst));
        let snapshot = metrics.snapshot(100, 1, false);
        assert!(!snapshot.post_roll_interrupted);
        assert_eq!(snapshot.post_roll_ms, 60);
    }

    #[test]
    fn later_stop_interrupts_post_roll() {
        let (stop_tx, stop_rx) = mpsc::channel();
        stop_tx
            .send(RecorderCommand::Stop {
                post_roll: Duration::ZERO,
            })
            .unwrap();
        let command = RecorderCommand::Stop {
            post_roll: Duration::from_secs(5),
        };
        let metrics = CaptureMetricsAccumulator::default();
        let stop_requested = AtomicBool::new(false);

        let start = Instant::now();
        stop_after_post_roll(Some(&command), &stop_rx, &metrics, &stop_requested);
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(stop_requested.load(Ordering::SeqCst));
        assert!(metrics.snapshot(100, 1, false).post_roll_interrupted);
    }

    #[test]
    fn disconnected_stop_channel_interrupts_post_roll() {
        let (stop_tx, stop_rx) = mpsc::channel::<RecorderCommand>();
        drop(stop_tx);
        let command = RecorderCommand::Stop {
            post_roll: Duration::from_secs(5),
        };
        let metrics = CaptureMetricsAccumulator::default();
        let stop_requested = AtomicBool::new(false);

        let start = Instant::now();
        stop_after_post_roll(Some(&command), &stop_rx, &metrics, &stop_requested);
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(stop_requested.load(Ordering::SeqCst));
        assert!(metrics.snapshot(100, 1, false).post_roll_interrupted);
    }

    #[test]
    fn capture_metrics_snapshot_includes_post_roll_speech() {
        let metrics = CaptureMetricsAccumulator::default();
        metrics.post_roll_ms.store(250, Ordering::SeqCst);
        metrics.post_roll_active.store(true, Ordering::SeqCst);
        metrics.observe(&[0.01, 0.01]);
        metrics.post_roll_active.store(false, Ordering::SeqCst);

        let snapshot = metrics.snapshot(100, 1, false);
        assert_eq!(snapshot.post_roll_ms, 250);
        assert!(!snapshot.post_roll_interrupted);
        assert!(snapshot.post_roll_speech_detected);

        let immediate = CaptureMetricsAccumulator::default();
        immediate.observe(&[0.01, 0.01]);
        let snapshot = immediate.snapshot(100, 1, false);
        assert_eq!(snapshot.post_roll_ms, 0);
        assert!(!snapshot.post_roll_interrupted);
        assert!(!snapshot.post_roll_speech_detected);
    }

    #[test]
    fn take_silence_event_receiver_consumes_receiver() {
        let mut recorder = AudioRecorder::new();
        let (_tx, rx) = mpsc::sync_channel::<SilenceDetectorEvent>(1);
        *recorder.silence_event_receiver.lock().unwrap() = Some(rx);

        assert!(recorder.take_silence_event_receiver().is_some());
        assert!(recorder.take_silence_event_receiver().is_none());
    }
    #[test]
    fn recording_thread_finished_is_false_when_idle() {
        let recorder = AudioRecorder::new();
        assert!(!recorder.recording_thread_finished());
    }

    #[test]
    fn recording_thread_finished_is_true_after_worker_self_exits() {
        let recorder = AudioRecorder::new();
        let (stop_tx, _stop_rx) = mpsc::channel::<RecorderCommand>();
        let thread_handle = thread::spawn(|| Ok::<String, String>("stopped".to_string()));

        let start = Instant::now();
        while !thread_handle.is_finished() {
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "worker never finished"
            );
            thread::sleep(Duration::from_millis(1));
        }

        *recorder.recording_handle.lock().unwrap() = Some(RecordingHandle {
            stop_tx,
            thread_handle,
        });

        assert!(recorder.recording_thread_finished());
    }

    #[test]
    fn recording_thread_finished_is_false_while_worker_runs() {
        let recorder = AudioRecorder::new();
        let (stop_tx, stop_rx) = mpsc::channel::<RecorderCommand>();
        let thread_handle = thread::spawn(move || {
            let _ = stop_rx.recv();
            Ok::<String, String>("stopped".to_string())
        });

        *recorder.recording_handle.lock().unwrap() = Some(RecordingHandle {
            stop_tx,
            thread_handle,
        });

        assert!(!recorder.recording_thread_finished());
        drop(recorder);
    }
    #[test]
    fn capture_metrics_accumulate_rms_peak_and_duration_in_callback_pass() {
        let metrics = CaptureMetricsAccumulator::default();

        assert!((metrics.observe(&[0.5, -0.5]) - 0.5).abs() < f32::EPSILON);
        assert!((metrics.observe(&[0.25, -0.25]) - 0.25).abs() < f32::EPSILON);

        let snapshot = metrics.snapshot(2, 1, true);
        assert_eq!(snapshot.sample_count, 4);
        assert_eq!(snapshot.duration_ms, 2000);
        assert!((snapshot.rms - 0.395_284_707_521_047_44).abs() < 1e-12);
        assert!((snapshot.peak - 0.5).abs() < f32::EPSILON);
        assert!(snapshot.speech_detected);
    }

    #[test]
    fn evidence_duration_uses_fixed_windows_across_callback_boundaries() {
        let samples: Vec<f32> = std::iter::repeat_n(0.004, 50)
            .chain(std::iter::repeat_n(0.0007, 50))
            .collect();
        let one_callback = CaptureMetricsAccumulator::new(1_000, 1);
        one_callback.observe(&samples);
        let one_snapshot = one_callback.snapshot(1_000, 1, false);

        let split_callbacks = CaptureMetricsAccumulator::new(1_000, 1);
        for chunk in samples.chunks(7) {
            split_callbacks.observe(chunk);
        }
        let split_snapshot = split_callbacks.snapshot(1_000, 1, false);

        assert_eq!(one_snapshot.ms_above_rms_floor, 50);
        assert_eq!(one_snapshot.windows_above_rms_floor, 10);
        assert_eq!(
            split_snapshot.ms_above_rms_floor,
            one_snapshot.ms_above_rms_floor
        );
        assert_eq!(
            split_snapshot.windows_above_rms_floor,
            one_snapshot.windows_above_rms_floor
        );
    }

    #[test]
    fn evidence_duration_counts_a_short_transient_not_its_host_callback() {
        let samples: Vec<f32> = std::iter::repeat_n(0.02, 5)
            .chain(std::iter::repeat_n(0.0007, 95))
            .collect();
        let metrics = CaptureMetricsAccumulator::new(1_000, 1);
        metrics.observe(&samples);

        let snapshot = metrics.snapshot(1_000, 1, false);
        assert_eq!(snapshot.ms_above_rms_floor, 5);
        assert_eq!(snapshot.windows_above_rms_floor, 1);
    }

    #[test]
    fn evidence_duration_rounds_up_past_transient_boundary() {
        let metrics = CaptureMetricsAccumulator::new(48_000, 1);
        let samples = vec![0.004; 961];
        metrics.observe(&samples);

        let snapshot = metrics.snapshot(48_000, 1, false);
        assert_eq!(snapshot.ms_above_rms_floor, 21);
    }

    #[test]
    fn soft_speech_survives_fixed_window_phase_alignment() {
        let metrics = CaptureMetricsAccumulator::new(2_000, 1);
        let samples: Vec<f32> = std::iter::repeat_n(0.0007, 5)
            .chain(std::iter::repeat_n(0.004, 62))
            .chain(std::iter::repeat_n(0.0007, 133))
            .collect();
        metrics.observe(&samples);

        let snapshot = metrics.snapshot(2_000, 1, false);
        assert!(
            snapshot.ms_above_rms_floor > 20,
            "31ms soft speech measured only {}ms above floor",
            snapshot.ms_above_rms_floor
        );
    }

    #[test]
    fn callback_rms_preserves_previous_f32_calculation_bits() {
        fn legacy_callback_rms(samples: &[f32]) -> f32 {
            let sum: f32 = samples.iter().map(|sample| sample * sample).sum();
            (sum / samples.len() as f32).sqrt()
        }

        let near_threshold = [
            crate::audio::silence_detector::VOICE_RMS_THRESHOLD - 0.000_001,
            crate::audio::silence_detector::VOICE_RMS_THRESHOLD + 0.000_001,
            -0.004_999,
            0.005_001,
        ];
        let signed_zero = [0.0, -0.0];

        for samples in [&[][..], &near_threshold[..], &signed_zero[..]] {
            let metrics = CaptureMetricsAccumulator::default();
            assert_eq!(
                metrics.observe(samples).to_bits(),
                legacy_callback_rms(samples).to_bits()
            );
        }
    }

    #[test]
    fn capture_metrics_survive_writer_and_device_errors() {
        let expected = CaptureAudioMetrics {
            sample_count: 16_000,
            start_to_first_audio_ms: None,
            duration_ms: 1000,
            rms: 0.1,
            peak: 0.2,
            max_window_rms: 0.1,
            windows_above_rms_floor: 0,
            ms_above_rms_floor: 0,
            sample_rate: 16_000,
            channels: 1,
            speech_detected: true,
            post_roll_ms: 0,
            post_roll_interrupted: false,
            post_roll_speech_detected: false,
        };

        for (writer_result, device_error, expected_error) in [
            (Err("writer failed".to_string()), None, "writer failed"),
            (Ok(()), Some("device failed".to_string()), "device failed"),
        ] {
            let slot = Mutex::new(None);
            let error = finish_capture(&slot, expected, writer_result, device_error).unwrap_err();
            assert_eq!(error, expected_error);
            assert_eq!(slot.lock().ok().and_then(|guard| *guard), Some(expected));
        }
    }

    #[test]
    fn take_last_capture_metrics_consumes_snapshot() {
        let recorder = AudioRecorder::new();
        let expected = CaptureAudioMetrics {
            sample_count: 16_000,
            start_to_first_audio_ms: None,
            duration_ms: 1000,
            rms: 0.1,
            peak: 0.2,
            max_window_rms: 0.1,
            windows_above_rms_floor: 0,
            ms_above_rms_floor: 0,
            sample_rate: 16_000,
            channels: 1,
            speech_detected: false,
            post_roll_ms: 0,
            post_roll_interrupted: false,
            post_roll_speech_detected: false,
        };
        if let Ok(mut guard) = recorder.last_capture_metrics.lock() {
            *guard = Some(expected);
        }

        assert_eq!(recorder.take_last_capture_metrics(), Some(expected));
        assert_eq!(recorder.take_last_capture_metrics(), None);
    }
    #[test]
    fn drain_final_callback_feeds_capture_evidence_from_the_final_buffer() {
        use crate::audio::speech_evidence::{classify_speech_evidence, SpeechEvidenceClass};
        // Plan 060.1: the stop boundary lands right after the last word — the
        // final buffer must contribute speech evidence, or the no-speech gate
        // deletes a recording whose speech the metrics never saw.
        let metrics = CaptureMetricsAccumulator::new(16_000, 1);
        // One second of quiet room tone. Nonzero on purpose: an all-zero
        // capture would take the HighConfidenceNoInput path, not the gate.
        metrics.observe(&vec![0.0007; 16_000]);
        // Pre-fix counterfactual: without the drained final buffer the
        // evidence classifies as high-confidence no-speech — would_skip_engine
        // is true and the recording (with its final word) is deleted.
        let baseline_only = metrics.snapshot(16_000, 1, false);
        assert!(matches!(
            classify_speech_evidence(Some(baseline_only), None),
            SpeechEvidenceClass::HighConfidenceNoSpeech
        ));

        // The final drained buffer: 35ms of soft speech (0.004 > the 0.003
        // window floor, yet below the aggregate floors) — past the 20ms
        // transient allowance the gate extends to a bare click.
        let final_buf: Vec<f32> = vec![0.004; 560];
        let i16_samples: Vec<i16> = final_buf.iter().map(|&sample| f32_to_i16(sample)).collect();

        let (pool_tx, pool_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        pool_tx.send(Vec::with_capacity(560)).unwrap();
        let (writer_tx, writer_rx) = mpsc::sync_channel::<WriterMsg>(1);
        let (recycle_tx, _recycle_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        let dropped = AtomicU64::new(0);

        drain_final_callback(
            &final_buf,
            &i16_samples,
            &metrics,
            &pool_rx,
            &writer_tx,
            &recycle_tx,
            &dropped,
            None,
        );

        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        let written = writer_rx.try_recv().ok();
        assert!(
            matches!(written.as_ref(), Some(WriterMsg::Chunk(chunk)) if *chunk == i16_samples),
            "final chunk must reach the writer verbatim"
        );
        // The gate can no longer skip this capture: the final buffer's
        // sustained above-floor windows pushed the evidence past the
        // transient threshold (engine runs, speech survives).
        let snapshot = metrics.snapshot(16_000, 1, false);
        let class = classify_speech_evidence(Some(snapshot), None);
        assert!(
            !class.would_skip_engine(),
            "final speech must not be gated away, got {class:?}"
        );
        assert!(matches!(class, SpeechEvidenceClass::Uncertain));
    }

    #[test]
    fn drain_final_callback_writes_the_final_chunk_when_the_pool_has_room() {
        let metrics = CaptureMetricsAccumulator::new(16_000, 1);
        let samples = vec![0.2f32; 160];
        let i16_samples: Vec<i16> = samples.iter().map(|&sample| f32_to_i16(sample)).collect();

        let (pool_tx, pool_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        pool_tx.send(Vec::with_capacity(160)).unwrap();
        let (writer_tx, writer_rx) = mpsc::sync_channel::<WriterMsg>(1);
        let (recycle_tx, _recycle_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        let dropped = AtomicU64::new(0);

        drain_final_callback(
            &samples,
            &i16_samples,
            &metrics,
            &pool_rx,
            &writer_tx,
            &recycle_tx,
            &dropped,
            None,
        );

        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        let written = writer_rx.try_recv().ok();
        assert!(
            matches!(written.as_ref(), Some(WriterMsg::Chunk(chunk)) if *chunk == i16_samples),
            "final chunk must reach the writer verbatim"
        );
    }

    #[test]
    fn drain_final_callback_counts_pool_exhaustion_as_dropped() {
        // Pre-fix the empty-pool path silently discarded the final buffer: the
        // WAV was truncated while the dropped counter stayed zero, so the
        // integrity check passed and the no-speech gate judged (and could
        // delete) a recording that never contained its own tail.
        let metrics = CaptureMetricsAccumulator::new(16_000, 1);
        let samples = vec![0.2f32; 160];
        let i16_samples: Vec<i16> = samples.iter().map(|&sample| f32_to_i16(sample)).collect();

        let (_pool_tx, pool_rx) = mpsc::sync_channel::<Vec<i16>>(1); // empty pool
        let (writer_tx, writer_rx) = mpsc::sync_channel::<WriterMsg>(1);
        let (recycle_tx, _recycle_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        let dropped = AtomicU64::new(0);

        drain_final_callback(
            &samples,
            &i16_samples,
            &metrics,
            &pool_rx,
            &writer_tx,
            &recycle_tx,
            &dropped,
            None,
        );

        assert_eq!(
            dropped.load(Ordering::SeqCst),
            1,
            "unwritable final buffer must count as dropped"
        );
        assert!(
            writer_rx.try_recv().is_err(),
            "nothing may reach the writer"
        );
    }

    #[test]
    fn drain_final_callback_counts_queue_full_and_disconnected_writer_as_dropped() {
        let metrics = CaptureMetricsAccumulator::new(16_000, 1);
        let samples = vec![0.2f32; 160];
        let i16_samples: Vec<i16> = samples.iter().map(|&sample| f32_to_i16(sample)).collect();

        // Full queue (occupied by a Finalize, the Windows stream-drop-timeout
        // case). The occupant is irrelevant: a Chunk send into a queue with
        // no free slot reports Full carrying OUR chunk, which returns to the
        // pool while the drop is counted.
        let (pool_tx, pool_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        pool_tx.send(Vec::with_capacity(160)).unwrap();
        let (writer_tx, _writer_rx) = mpsc::sync_channel::<WriterMsg>(1);
        writer_tx.send(WriterMsg::Finalize).unwrap();
        let (recycle_tx, recycle_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        let dropped = AtomicU64::new(0);

        drain_final_callback(
            &samples,
            &i16_samples,
            &metrics,
            &pool_rx,
            &writer_tx,
            &recycle_tx,
            &dropped,
            None,
        );
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert!(
            recycle_rx.try_recv().is_ok(),
            "the borrowed chunk returns to the pool for the RT thread"
        );

        // Disconnected writer (receiver dropped) with a pool chunk available:
        // the send itself fails, and the drop is counted the same way.
        let (pool_tx, pool_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        pool_tx.send(Vec::with_capacity(160)).unwrap();
        let (writer_tx, writer_rx) = mpsc::sync_channel::<WriterMsg>(1);
        drop(writer_rx);
        let dropped = AtomicU64::new(0);
        drain_final_callback(
            &samples,
            &i16_samples,
            &metrics,
            &pool_rx,
            &writer_tx,
            &recycle_tx,
            &dropped,
            None,
        );
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn drain_final_callback_counts_full_writer_queue_as_dropped_and_recycles() {
        // Queue occupied by a Chunk (the Full(Chunk) case): the buffer is
        // counted dropped and the recycled chunk returns to the pool for the
        // real-time thread.
        let metrics = CaptureMetricsAccumulator::new(16_000, 1);
        let samples = vec![0.2f32; 160];
        let i16_samples: Vec<i16> = samples.iter().map(|&sample| f32_to_i16(sample)).collect();

        let (pool_tx, pool_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        pool_tx.send(Vec::with_capacity(160)).unwrap();
        let (writer_tx, _writer_rx) = mpsc::sync_channel::<WriterMsg>(1);
        writer_tx.send(WriterMsg::Chunk(Vec::new())).unwrap(); // occupy the queue
        let (recycle_tx, recycle_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        let dropped = AtomicU64::new(0);

        drain_final_callback(
            &samples,
            &i16_samples,
            &metrics,
            &pool_rx,
            &writer_tx,
            &recycle_tx,
            &dropped,
            None,
        );
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert!(
            recycle_rx.try_recv().is_ok(),
            "Full(Chunk) returns its chunk to the pool"
        );
    }
}
