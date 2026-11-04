//! One unlocked emitter; publications and removals always share its FIFO.
use super::{replay, retire_queued_kept_id, withdraw_queued_live_mic_silent};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

type Event = (&'static str, Value);

pub(super) struct Outbox {
    pub pending: Vec<Event>,
    pub ready: bool,
    flushing: bool,
    requested: bool,
    had_pending: bool,
    produced_ui: bool,
    renderer_has_ui: bool,
    epoch: u64,
    rebuild: bool,
    statuses: Vec<Event>,
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
            renderer_has_ui: false,
            epoch: 0,
            rebuild: false,
            statuses: Vec::new(),
        }
    }

    pub fn enqueue(&mut self, event: &'static str, payload: Value) {
        self.remember(event, &payload);
        // Before flushing, old candidates can be evicted. During flushing a
        // removal may target an in-flight batch, so it must never be evicted.
        if !self.flushing && self.pending.len() >= 32 {
            self.pending.remove(0);
        }
        self.pending.push((event, payload));
    }

    /// Only generation-scoped mic statuses/blockers need retained publication
    /// metadata. Recovery ownership always comes from the live kept store.
    pub fn remember(&mut self, event: &'static str, payload: &Value) {
        if event != "dictation-blocked"
            && !(event == "dictation-note"
                && payload["kind"]
                    .as_str()
                    .is_some_and(|kind| kind.starts_with("mic_")))
        {
            return;
        }
        self.statuses.retain(|(old_event, old)| {
            *old_event != event
                || old["kind"] != payload["kind"]
                || old["generation"] != payload["generation"]
        });
        if self.statuses.len() == 32 {
            self.statuses.remove(0);
        }
        self.statuses.push((event, payload.clone()));
    }

    pub fn rebuild_from_live(&mut self, recoveries: &[Value]) {
        if !self.rebuild || !self.requested {
            return;
        }
        self.rebuild = false;
        let mut seed = self.statuses.clone();
        seed.extend(
            recoveries
                .iter()
                .cloned()
                .map(|value| ("dictation-recovery", value)),
        );
        seed.append(&mut self.pending);
        self.pending = seed;
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn accepts_visibility(&self, epoch: u64) -> bool {
        self.epoch == epoch && self.requested
    }

    pub fn visibility(&mut self, epoch: u64, visible: bool) -> bool {
        if !self.accepts_visibility(epoch) {
            return false;
        }
        self.renderer_has_ui = visible;
        true
    }

    pub fn tag(&self, mut payload: Value) -> Value {
        payload["renderer_epoch"] = self.epoch.into();
        payload
    }

    /// Claim the emitter atomically with a ready-state publication/removal.
    /// In-flight sends keep ready false; their successors join the next batch.
    pub fn resume(&mut self) -> bool {
        if !self.ready {
            return false;
        }
        self.ready = false;
        let start = self.begin_flush();
        // A removal-only live batch must not release another rendered card.
        // Once the renderer reports empty, stale candidates can release again.
        self.produced_ui = self.renderer_has_ui;
        start
    }

    pub fn begin_flush_for(&mut self, epoch: u64) -> bool {
        self.epoch == epoch && self.begin_flush()
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
            // Empty observation and release of the emitter slot are atomic.
            self.ready = true;
            self.flushing = false;
            return None;
        }
        self.had_pending = true;
        Some(std::mem::take(&mut self.pending))
    }

    pub fn validated(&mut self, batch: &[Event], owned: &AtomicBool) {
        let visual = batch.iter().any(|(event, _)| replay::produces_ui(event));
        self.produced_ui |= visual;
        self.renderer_has_ui |= visual;
        if visual {
            owned.store(true, Ordering::SeqCst);
        }
    }

    pub fn withdraw(&mut self, generation: u64, owned: &AtomicBool) {
        withdraw_queued_live_mic_silent(&mut self.statuses, generation, true, owned);
        withdraw_queued_live_mic_silent(
            &mut self.pending,
            generation,
            self.ready || self.flushing,
            owned,
        );
        self.enqueue(
            "dictation-note-withdraw",
            json!({
                "generation":generation, "kind":"mic_silent_live"
            }),
        );
    }

    pub fn retire(&mut self, id: &str, owned: &AtomicBool) {
        retire_queued_kept_id(&mut self.pending, id, self.ready || self.flushing, owned);
        self.enqueue("kept-dictation-expired", json!({"id":id}));
    }

    pub fn close(&mut self, owned: &AtomicBool) {
        self.epoch += 1;
        self.rebuild = true;
        self.ready = false;
        self.requested = false;
        self.pending.clear();
        self.had_pending = false;
        self.produced_ui = false;
        self.renderer_has_ui = false;
        owned.store(false, Ordering::SeqCst);
        // An in-flight drainer still owns the emit slot. A subsequent ready
        // call requests another pass rather than starting a second emitter.
    }
}

#[cfg(test)]
mod epochs;
#[cfg(test)]
mod tests;
