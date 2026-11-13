//! Ephemeral, generation-owned nearby text. Nothing in this module enters History.
pub(crate) mod cli;
mod hint;
#[cfg(target_os = "macos")]
mod macos;
mod session;

pub(crate) use session::{begin, cancel, discard, reference_for_polish, reset, scope, stop};

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Available,
    Partial,
    Disabled,
    Secure,
    Denied,
    Unsupported,
    Timeout,
    TargetChanged,
}

/// Moves with a take's transcription future and clears unused context on every
/// terminal path, including aborting the future before polling it.
pub(crate) struct Finish(pub(crate) u64);
impl Drop for Finish {
    fn drop(&mut self) {
        discard(self.0);
    }
}

pub(crate) const BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

// Only one native operation may be outstanding, including one whose deadline
// expired. A stalled OS provider cannot cause an accumulation of detached work.
#[cfg(target_os = "macos")]
static WORKER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "macos")]
struct Worker;
#[cfg(target_os = "macos")]
impl Worker {
    fn claim() -> Option<Self> {
        WORKER
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .ok()
            .map(|_| Self)
    }
}
#[cfg(target_os = "macos")]
impl Drop for Worker {
    fn drop(&mut self) {
        WORKER.store(false, std::sync::atomic::Ordering::Release);
    }
}
