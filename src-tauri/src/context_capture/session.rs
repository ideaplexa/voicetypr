#[cfg(target_os = "macos")]
use super::Worker;
use super::{Status, BUDGET};
use crate::ai::context_reference::ContextReference;
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex,
    },
    time::Instant,
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_store::StoreExt;

struct Session {
    generation: u64,
    cancellation: Arc<AtomicBool>,
    status: Status,
    #[cfg(target_os = "macos")]
    captured: Option<super::macos::Captured>,
}
impl Session {
    fn admit(&mut self, generation: u64, enabled: bool) -> Option<Arc<AtomicBool>> {
        if self.generation != generation || self.cancellation.load(Ordering::Acquire) {
            return None;
        }
        self.status = if enabled {
            Status::Timeout
        } else {
            Status::Disabled
        };
        Some(self.cancellation.clone())
    }
}
static SESSION: LazyLock<Mutex<Option<Session>>> = LazyLock::new(|| Mutex::new(None));
tokio::task_local! { static GENERATION: u64; }

pub(crate) fn reset(generation: u64) {
    if let Ok(mut slot) = SESSION.lock() {
        if let Some(previous) = slot.take() {
            previous.cancellation.store(true, Ordering::Release);
        }
        *slot = Some(Session {
            generation,
            cancellation: Arc::new(AtomicBool::new(false)),
            status: Status::Disabled,
            #[cfg(target_os = "macos")]
            captured: None,
        });
    }
}

pub(crate) async fn scope<F: Future>(generation: u64, future: F) -> F::Output {
    GENERATION.scope(generation, future).await
}

pub(crate) fn cancel() {
    if let Ok(mut slot) = SESSION.lock() {
        if let Some(previous) = slot.take() {
            previous.cancellation.store(true, Ordering::Release);
        }
    }
}
pub(crate) fn discard(generation: u64) {
    if let Ok(mut slot) = SESSION.lock() {
        if slot.as_ref().is_some_and(|s| s.generation == generation) {
            if let Some(previous) = slot.take() {
                previous.cancellation.store(true, Ordering::Release);
            }
        }
    }
}
// Stop cancels outstanding reads, not an already completed snapshot needed by
// this take's Polish. New takes and discard erase both.
pub(crate) fn stop(generation: u64) {
    if let Ok(slot) = SESSION.lock() {
        if let Some(session) = slot.as_ref().filter(|s| s.generation == generation) {
            session.cancellation.store(true, Ordering::Release);
        }
    }
}

pub(crate) fn begin(app: &AppHandle, generation: u64) {
    if crate::commands::audio::recording_generation_is_stale(generation) {
        return;
    }
    super::hint::spawn(app, generation);
    let started = Instant::now();
    let store = app.store("settings").ok();
    let enabled = store.as_ref().is_some_and(|s| {
        s.get("ai_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && s.get("polish_context_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
    });
    let provider = store
        .as_ref()
        .and_then(|s| s.get("ai_provider"))
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    // Custom endpoints may be local; use their stricter budget conservatively.
    let local = provider == "custom";
    let cancellation = {
        let Ok(mut slot) = SESSION.lock() else {
            return;
        };
        if crate::commands::audio::recording_generation_is_stale(generation) {
            return;
        }
        let Some(cancellation) = slot.as_mut().and_then(|s| s.admit(generation, enabled)) else {
            return;
        };
        cancellation
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if enabled {
            let deadline = started + BUDGET;
            #[cfg(target_os = "macos")]
            let result = if let Some(worker) = Worker::claim() {
                let cancelled = cancellation.clone();
                tokio::time::timeout_at(
                    tokio::time::Instant::from_std(deadline),
                    tauri::async_runtime::spawn_blocking(move || {
                        let _worker = worker;
                        super::macos::capture(local, deadline, &cancelled)
                    }),
                )
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or(Err(Status::Timeout))
            } else {
                Err(Status::Timeout)
            };
            #[cfg(not(target_os = "macos"))]
            let result: Result<(), Status> = {
                let _ = (local, deadline);
                Err(Status::Unsupported)
            };
            if let Ok(mut slot) = SESSION.lock() {
                if let Some(session) = slot.as_mut().filter(|s| s.generation == generation) {
                    if !cancellation.load(Ordering::Acquire) && Instant::now() <= deadline {
                        match result {
                            #[cfg(target_os = "macos")]
                            Ok(captured) => {
                                session.status = if captured.partial {
                                    Status::Partial
                                } else {
                                    Status::Available
                                };
                                session.captured = Some(captured);
                            }
                            #[cfg(not(target_os = "macos"))]
                            Ok(()) => {}
                            Err(status) => session.status = status,
                        }
                    }
                }
            }
        }
        let status = SESSION.lock().ok().and_then(|slot| {
            slot.as_ref()
                .filter(|s| s.generation == generation)
                .map(|s| s.status)
        });
        if let Some(status) = status {
            let _ = app.emit(
                "polish-context-status",
                serde_json::json!({"generation": generation, "status": status}),
            );
        }
    });
}

pub(crate) async fn reference_for_polish(app: &AppHandle) -> Option<ContextReference> {
    let generation = GENERATION.try_with(|g| *g).ok().filter(|g| *g != 0)?;
    if crate::commands::audio::recording_generation_is_stale(generation) {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        // Move the snapshot to a bounded worker, never hold a session lock during AX IPC.
        let captured = {
            let mut slot = SESSION.lock().ok()?;
            slot.as_mut()
                .filter(|s| s.generation == generation)?
                .captured
                .take()?
        };
        let worker = Worker::claim()?;
        let deadline = Instant::now() + BUDGET;
        let reference = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            tauri::async_runtime::spawn_blocking(move || {
                let _worker = worker;
                super::macos::validate(&captured.target, deadline)?;
                Ok(captured.reference)
            }),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(Err(Status::Timeout));
        // Recheck after the worker: new-generation publication may have raced AX.
        let mut slot = SESSION.lock().ok()?;
        let session = slot.as_mut().filter(|s| s.generation == generation)?;
        if crate::commands::audio::recording_generation_is_stale(generation) {
            return None;
        }
        match reference {
            Ok(reference) => Some(reference),
            Err(status) => {
                session.status = status;
                let _ = app.emit(
                    "polish-context-status",
                    serde_json::json!({"generation": generation, "status": status}),
                );
                None
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(generation: u64) -> Session {
        Session {
            generation,
            cancellation: Arc::new(AtomicBool::new(false)),
            status: Status::Disabled,
            #[cfg(target_os = "macos")]
            captured: None,
        }
    }

    #[test]
    fn stale_and_stopped_generations_cannot_start_native_reads() {
        let mut take = session(9);
        assert!(take.admit(8, true).is_none());
        let cancellation = take.admit(9, true).unwrap();
        cancellation.store(true, Ordering::Release);
        assert!(take.admit(9, true).is_none());
    }
}
