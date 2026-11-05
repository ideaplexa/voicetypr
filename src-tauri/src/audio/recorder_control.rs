//! Recorder lifecycle control, separate from the real-time audio callback.
use super::CaptureMetricsAccumulator;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, Notify};

/// One capture's monotonic stop latch. It never targets a later generation and
/// does not need the recorder mutex, including while that mutex joins a worker.
#[derive(Clone, Default)]
pub struct CaptureInterrupt(Arc<InterruptState>);

#[derive(Default)]
struct InterruptState {
    interrupted: AtomicBool,
    stop_tx: Mutex<Option<mpsc::Sender<RecorderCommand>>>,
    wake: Notify,
}

impl CaptureInterrupt {
    /// Only the atomic load is used by the real-time callback.
    pub(crate) fn is_interrupted(&self) -> bool {
        self.0.interrupted.load(Ordering::SeqCst)
    }

    pub(crate) fn interrupt(&self) {
        if self.0.interrupted.swap(true, Ordering::SeqCst) {
            return;
        }
        self.0.wake.notify_waiters();
        if let Some(tx) = self
            .0
            .stop_tx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            let _ = tx.send(RecorderCommand::Stop {
                post_roll: Duration::ZERO,
            });
        }
    }

    pub(super) fn attach(&self, tx: mpsc::Sender<RecorderCommand>) {
        let mut sender = self.0.stop_tx.lock().unwrap_or_else(|e| e.into_inner());
        // An interruption during settings work wins even before a worker exists.
        if self.is_interrupted() {
            let _ = tx.send(RecorderCommand::Stop {
                post_roll: Duration::ZERO,
            });
        }
        *sender = Some(tx);
    }

    /// Native initialization may already be in flight, but it must not proceed
    /// to the next phase (especially play) once the capture is interrupted.
    pub(super) fn run_if_active<T, E>(
        &self,
        operation: impl FnOnce() -> Result<T, E>,
    ) -> Result<Option<T>, E> {
        if self.is_interrupted() {
            Ok(None)
        } else {
            operation().map(Some)
        }
    }

    async fn wait(&self) {
        let notified = self.0.wake.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.is_interrupted() {
            notified.await;
        }
    }
}

pub(crate) async fn wait_for_readiness(
    readiness: oneshot::Receiver<RecordingReadiness>,
    interrupt: &CaptureInterrupt,
) -> Option<RecordingReadiness> {
    tokio::select! {
        biased;
        _ = interrupt.wait() => Some(RecordingReadiness::Failed(
            "Recording interrupted before microphone readiness".to_string(),
        )),
        result = tokio::time::timeout(Duration::from_secs(2), readiness) => match result {
            Ok(Ok(readiness)) => Some(readiness),
            Ok(Err(_)) => Some(RecordingReadiness::Failed(
                "Recorder stopped before reporting microphone readiness".to_string(),
            )),
            Err(_) => None,
        },
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interruption_before_worker_registration_is_latched_without_constructing_capture() {
        let interrupt = CaptureInterrupt::default();
        interrupt.interrupt();
        let (tx, rx) = mpsc::channel();
        interrupt.attach(tx);
        assert!(matches!(rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            RecorderCommand::Stop { post_roll } if post_roll.is_zero()));
        let result: Result<Option<()>, ()> =
            interrupt.run_if_active(|| panic!("must not construct capture"));
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn interruption_during_native_build_prevents_play() {
        let interrupt = CaptureInterrupt::default();
        let worker_interrupt = interrupt.clone();
        let (built_tx, built_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let built: Result<Option<()>, ()> = worker_interrupt.run_if_active(|| {
                built_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                Ok(())
            });
            assert_eq!(built, Ok(Some(())));
            let played: Result<Option<()>, ()> =
                worker_interrupt.run_if_active(|| panic!("must not play"));
            assert_eq!(played, Ok(None));
        });
        built_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        interrupt.interrupt();
        release_tx.send(()).unwrap();
        worker.join().unwrap();
    }

    #[tokio::test]
    async fn interruption_wakes_readiness_instead_of_waiting_for_its_two_second_timeout() {
        let interrupt = CaptureInterrupt::default();
        let (ready_tx, ready_rx) = oneshot::channel();
        let waiting_interrupt = interrupt.clone();
        let waiter =
            tokio::spawn(async move { wait_for_readiness(ready_rx, &waiting_interrupt).await });
        tokio::task::yield_now().await;
        interrupt.interrupt();
        let readiness = tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(readiness, Some(RecordingReadiness::Failed(_))));
        drop(ready_tx);
    }

    #[test]
    fn secure_input_entry_cuts_in_progress_post_roll_without_discarding_or_retargeting() {
        use crate::recording::secure_input::{interrupt_capture, post_roll};
        let state = crate::AppState::new();
        for phase in [
            crate::RecordingState::Starting,
            crate::RecordingState::Recording,
            crate::RecordingState::Stopping,
        ] {
            state.transition_recording_state(phase).unwrap();
        }
        let generation = 7;
        let interrupt = state.capture_interrupts.begin(generation);
        let (tx, rx) = mpsc::channel();
        interrupt.attach(tx);
        let metrics = Arc::new(CaptureMetricsAccumulator::new(48_000, 1));
        let worker_metrics = metrics.clone();
        let stop_requested = Arc::new(AtomicBool::new(false));
        let worker_stop = stop_requested.clone();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let user_stop = RecorderCommand::Stop {
                post_roll: Duration::from_secs(5),
            };
            stop_after_post_roll(Some(&user_stop), &rx, &worker_metrics, &worker_stop);
            done_tx.send(()).unwrap();
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while !metrics.post_roll_active.load(Ordering::SeqCst) {
            assert!(
                Instant::now() < deadline,
                "post-roll worker did not enter its wait"
            );
            std::thread::yield_now();
        }
        let signalled = interrupt_capture(&state, generation, true, false);
        let cut_promptly = done_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        // Always release our test worker, including on a regressed state filter.
        if !cut_promptly {
            interrupt.interrupt();
        }
        worker.join().unwrap();
        assert!(signalled);
        assert!(
            cut_promptly,
            "Secure Input must cut the active post-roll wait"
        );
        assert!(stop_requested.load(Ordering::SeqCst));
        assert!(metrics.post_roll_interrupted.load(Ordering::SeqCst));
        assert_eq!(state.get_current_state(), crate::RecordingState::Stopping);
        assert!(
            !state.is_cancellation_requested(),
            "keep the captured prefix for its existing transcription"
        );
        assert!(!state.pending_stop_after_start.load(Ordering::SeqCst));
        let next = state.capture_interrupts.begin(generation + 1);
        let (next_tx, next_rx) = mpsc::channel();
        next.attach(next_tx);
        interrupt_capture(&state, generation, true, false);
        assert!(!next.is_interrupted());
        assert!(matches!(next_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert_eq!(
            post_roll(&state, generation + 1, Duration::from_millis(250)),
            Duration::from_millis(250)
        );
    }
}
