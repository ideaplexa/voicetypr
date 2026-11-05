//! Single outstanding job, no queue, no joins on any recording/transcription path.
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};

use super::silero_vad::{self, Summary};
use super::speech_evidence::SpeechEvidenceClass;

static BUSY: AtomicBool = AtomicBool::new(false);
static WORKERS: Workers = Workers::new();

struct WorkerState {
    stopping: bool,
    active: usize,
}
struct Workers {
    state: Mutex<WorkerState>,
    changed: Condvar,
}
struct WorkerLease<'a>(&'a Workers);
impl Workers {
    const fn new() -> Self {
        Self {
            state: Mutex::new(WorkerState {
                stopping: false,
                active: 0,
            }),
            changed: Condvar::new(),
        }
    }
    fn enter(&self) -> Option<WorkerLease<'_>> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopping {
            return None;
        }
        state.active += 1;
        Some(WorkerLease(self))
    }
    fn shutdown(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopping = true;
        self.changed.notify_all();
        while state.active != 0 {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}
impl Drop for WorkerLease<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        self.0.changed.notify_all();
    }
}

/// Exit only: prevent new native work/reports and drain any existing session
/// before analytics/runtime teardown. Never called on a recording path.
/// A native call cannot be killed safely, so quit waits for it to unwind.
pub fn shutdown() {
    WORKERS.shutdown();
}

struct Slot(&'static AtomicBool);
impl Slot {
    fn acquire(busy: &'static AtomicBool) -> Option<Self> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(busy))
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub struct Observation {
    file: Option<File>,
    slot: Option<Slot>,
    status: &'static str,
    generation: u64,
    epoch: Option<u64>,
    trace_id: Option<String>,
    gate_skipped: bool,
}

impl Observation {
    /// Open the original raw WAV before normalization/deletion. The OS handle,
    /// not a second audio copy or a path lease, survives unlink. No audio is saved.
    pub fn new(path: &Path, generation: u64, gate_skipped: bool) -> Self {
        let slot = Slot::acquire(&BUSY);
        let file = slot.as_ref().and_then(|_| File::open(path).ok());
        let status = if slot.is_none() {
            "busy"
        } else {
            "unavailable"
        };
        Self {
            file,
            slot,
            status,
            generation,
            gate_skipped,
            epoch: crate::product_analytics::consent_epoch(),
            trace_id: crate::observability::trace_id(generation),
        }
    }

    /// Called by the attempt finalizer, after outcome/delivery, never by CPAL.
    /// Return type is deliberately unit: the summary cannot influence dictation.
    pub fn finish(self, class: SpeechEvidenceClass) {
        let Some(worker) = WORKERS.enter() else {
            return;
        };
        let generation = self.generation;
        let trace_id = self.trace_id;
        let epoch = self.epoch;
        let gate_skipped = self.gate_skipped;
        let report = std::sync::Arc::new(move |summary: Summary| {
            let agreement = match summary.speech_detected() {
                Some(speech) if speech != gate_skipped => "agree",
                Some(_) => "disagree",
                None => "unknown",
            };
            let properties = vec![
                ("status", summary.status.into()),
                ("current_gate", class.as_str().into()),
                ("gate_skipped", u64::from(gate_skipped).into()),
                ("agreement", agreement.into()),
                ("runtime_ms", summary.runtime_ms.into()),
                ("frames", summary.frames.into()),
                ("analyzed_ms", summary.analyzed_ms.into()),
                ("speech_ms", summary.speech_ms.into()),
                (
                    "probability_mean_milli",
                    summary.probability_mean_milli.into(),
                ),
                (
                    "probability_max_milli",
                    summary.probability_max_milli.into(),
                ),
            ];
            log::info!(
                "VAD_SHADOW {}",
                serde_json::json!({
                    "summary": summary, "current_gate": class.as_str(),
                    "gate_skipped": gate_skipped, "agreement": agreement,
                })
            );
            if epoch.is_none() {
                return;
            }
            crate::product_analytics::capture_with_trace(
                crate::product_analytics::ProductEvent::Observability {
                    name: "vad_shadow",
                    properties,
                },
                generation,
                epoch,
                trace_id.clone(),
            );
        });
        let Some(file) = self.file else {
            report(Summary::skipped(self.status));
            return;
        };
        let slot = self.slot;
        let failed = report.clone();
        // Spawn failure drops the closure, closing the handle and releasing the slot.
        // No native error strings enter either telemetry or the log.
        if std::thread::Builder::new()
            .name("vad-shadow".into())
            .spawn(move || {
                let _worker = worker;
                let _slot = slot;
                let summary = std::panic::catch_unwind(|| {
                    silero_vad::analyze(BufReader::new(file), silero_vad::BUDGET)
                })
                .unwrap_or_else(|_| Summary::skipped("unavailable"));
                report(summary);
            })
            .is_err()
        {
            // The rejected spawn already dropped its lease. Re-enter so a
            // concurrent quit cannot tear analytics down during this report.
            if let Some(_worker) = WORKERS.enter() {
                failed(Summary::skipped("unavailable"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quit_drains_inflight_native_work_and_refuses_late_finalizers() {
        let workers = Workers::new();
        let lease = workers.enter().unwrap();
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                workers.shutdown();
                done.send(()).unwrap();
            });
            let mut state = workers.state.lock().unwrap();
            while !state.stopping {
                state = workers.changed.wait(state).unwrap();
            }
            assert_eq!(state.active, 1);
            drop(state);
            assert!(workers.enter().is_none());
            assert!(finished.try_recv().is_err());
            drop(lease);
            finished.recv().unwrap();
            assert!(workers.enter().is_none());
        });
    }

    #[test]
    fn recorder_callback_has_no_inference_adapter() {
        let recorder = include_str!("recorder.rs");
        assert!(!recorder.contains("silero_vad"));
        assert!(!recorder.contains("vad_shadow"));
    }

    #[test]
    fn raw_file_handle_survives_deletion_without_retaining_a_path_lease() {
        use std::io::Read;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("capture.wav");
        std::fs::write(&path, b"raw capture").unwrap();
        let mut file = File::open(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"raw capture");
        assert!(!path.exists());
    }

    #[test]
    fn slot_is_bounded_and_released_on_drop() {
        static TEST_BUSY: AtomicBool = AtomicBool::new(false);
        let slot = Slot::acquire(&TEST_BUSY).unwrap();
        assert!(Slot::acquire(&TEST_BUSY).is_none());
        drop(slot);
        assert!(Slot::acquire(&TEST_BUSY).is_some());
    }
}
