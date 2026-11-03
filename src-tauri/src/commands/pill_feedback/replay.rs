//! A pre-ready event is a candidate, not proof that its UI is still live.
use crate::{recording::island::BlockedKind, RecordingState};
use serde_json::Value;
use std::collections::HashSet;

type Event = (&'static str, Value);

pub(super) fn produces_ui(event: &str) -> bool {
    !matches!(
        event,
        "island-notice-clear" | "dictation-note-withdraw" | "kept-dictation-expired"
    )
}

pub(super) fn release_nonvisual_replay(replay: &[Event], owned: &std::sync::atomic::AtomicBool) {
    if !replay.iter().any(|(event, _)| produces_ui(event)) {
        owned.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub(super) fn revalidate(
    queued: Vec<Event>,
    generation: u64,
    state: RecordingState,
    blocked: Option<BlockedKind>,
    recoveries: &[Value],
) -> Vec<Event> {
    let blocked = serde_json::to_value(blocked).unwrap();
    let mut seen = HashSet::new();
    let mut replay = Vec::new();
    // Last publication wins per card kind, or per opaque recovery id. Distinct
    // kept clips must not supersede each other's retry actions.
    for (event, mut payload) in queued.into_iter().rev() {
        let key = if event == "dictation-recovery" {
            payload["id"].as_str().unwrap_or_default()
        } else {
            payload["kind"].as_str().unwrap_or_default()
        }
        .to_owned();
        let family = if event == "island-notice-clear" {
            "island-notice"
        } else {
            event
        };
        if event == "dictation-recovery" {
            // Compare all actionable metadata, not only the id: a failed retry
            // can republish the same clip with another generation/alternative.
            let Some(live) = recoveries.iter().find(|live| {
                live["id"].as_str().is_some()
                    && live.as_object().is_some_and(|fields| {
                        fields
                            .iter()
                            .filter(|(key, _)| key.as_str() != "expires_in_ms")
                            .all(|(key, value)| payload.get(key) == Some(value))
                    })
            }) else {
                continue;
            };
            if live["kind"] == "mic_dropped_empty"
                && live["generation"].as_u64() != Some(generation)
            {
                continue;
            }
            // Do not restart the original TTL when the renderer was slow to boot.
            payload = live.clone();
        } else {
            if payload["generation"]
                .as_u64()
                .is_some_and(|queued_generation| queued_generation != generation)
            {
                continue;
            }
            if event == "dictation-blocked" && payload["kind"] != blocked {
                continue;
            }
            if event == "dictation-note"
                && payload["kind"] == "mic_silent_live"
                && state != RecordingState::Recording
            {
                continue;
            }
        }
        if seen.insert((family, key)) {
            replay.push((event, payload));
        }
    }
    replay.reverse();
    replay
}

#[cfg(test)]
mod tests;
