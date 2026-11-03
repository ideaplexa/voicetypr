//! One pre-ready drainer; publications and removals share its FIFO.
use super::{replay, retire_queued_kept_id, withdraw_queued_live_mic_silent};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;

type Event = (&'static str, Value);

pub(super) struct Outbox {
    pub pending: Vec<Event>,
    pub ready: bool,
    flushing: bool,
    requested: bool,
    had_pending: bool,
    produced_ui: bool,
}

impl Outbox {
    pub const fn new() -> Self {
        Self {
            pending: Vec::new(),
            ready: false,
            flushing: false,
            requested: false,
            had_pending: false,
            produced_ui: false,
        }
    }

    pub fn enqueue(&mut self, event: &'static str, payload: Value) {
        // Before flushing, old candidates can be evicted. During flushing a
        // removal may target an in-flight batch, so it must never be evicted.
        if !self.flushing && self.pending.len() >= 32 {
            self.pending.remove(0);
        }
        self.pending.push((event, payload));
    }

    pub fn begin_flush(&mut self) -> bool {
        if self.ready {
            return false;
        }
        self.requested = true;
        if self.flushing {
            return false;
        }
        self.flushing = true;
        self.had_pending = false;
        self.produced_ui = false;
        true
    }

    /// Called under the queue lock, only AFTER emitting the preceding batch.
    pub fn take_batch(&mut self, owned: &AtomicBool) -> Option<Vec<Event>> {
        if !self.requested {
            self.flushing = false;
            return None;
        }
        if self.pending.is_empty() {
            if self.had_pending && !self.produced_ui {
                replay::release_nonvisual_replay(&[], owned);
            }
            // The empty observation and the switch to direct sends are atomic.
            self.ready = true;
            self.flushing = false;
            return None;
        }
        self.had_pending = true;
        Some(std::mem::take(&mut self.pending))
    }

    pub fn validated(&mut self, batch: &[Event]) {
        self.produced_ui |= batch.iter().any(|(event, _)| replay::produces_ui(event));
    }

    pub fn withdraw(&mut self, generation: u64, owned: &AtomicBool) {
        withdraw_queued_live_mic_silent(
            &mut self.pending,
            generation,
            self.ready || self.flushing,
            owned,
        );
        if !self.ready {
            self.enqueue(
                "dictation-note-withdraw",
                json!({
                    "generation":generation, "kind":"mic_silent_live"
                }),
            );
        }
    }

    pub fn retire(&mut self, id: &str, owned: &AtomicBool) {
        retire_queued_kept_id(&mut self.pending, id, self.ready || self.flushing, owned);
        if !self.ready {
            self.enqueue("kept-dictation-expired", json!({"id":id}));
        }
    }

    pub fn close(&mut self) {
        self.ready = false;
        self.requested = false;
        // An in-flight drainer still owns the emit slot. A subsequent ready
        // call requests another pass rather than starting a second emitter.
    }
}

#[cfg(test)]
mod tests;
