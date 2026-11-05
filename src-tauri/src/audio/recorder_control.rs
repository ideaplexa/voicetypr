//! Recorder lifecycle control, separate from the real-time audio callback.
use super::CaptureMetricsAccumulator;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum RecordingReadiness {
    Ready { first_audio_ms: u64 },
    Failed(String),
}

pub(super) struct ReadinessReporter {
    pub(super) tx: Option<oneshot::Sender<RecordingReadiness>>,
}

impl Drop for ReadinessReporter {
    fn drop(&mut self) {
        report_start_failure(&mut self.tx, "Microphone initialization failed");
    }
}

pub(super) fn report_first_callback(
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

pub(super) fn report_start_failure(
    ready_tx: &mut Option<oneshot::Sender<RecordingReadiness>>,
    error: &str,
) {
    if let Some(tx) = ready_tx.take() {
        let _ = tx.send(RecordingReadiness::Failed(error.to_string()));
    }
}

#[derive(Debug)]
pub(super) enum RecorderCommand {
    Stop { post_roll: Duration },
}

pub(super) fn stop_after_post_roll(
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
