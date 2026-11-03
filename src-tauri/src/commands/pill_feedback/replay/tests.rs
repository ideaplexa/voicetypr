use super::*;
use serde_json::json;
use std::sync::atomic::AtomicBool;

fn note(generation: u64, kind: &str) -> Event {
    (
        "dictation-note",
        json!({"generation":generation,"kind":kind}),
    )
}

fn recovery(generation: u64, id: &str, kind: &str) -> Value {
    json!({
        "generation":generation, "id":id, "kind":kind,
        "engine_short":"Whisper", "alt_engine_short":"Parakeet",
        "alt_engine_id":"parakeet-tdt-0.6b-v3", "expires_in_ms":600_000
    })
}

#[test]
fn flush_drops_mic_statuses_from_older_generations() {
    let queued = vec![
        note(7, "mic_silent"),
        note(7, "mic_silent_live"),
        note(7, "mic_dropped"),
        (
            "dictation-blocked",
            json!({"generation":7,"kind":"mic_missing"}),
        ),
    ];
    assert!(revalidate(
        queued,
        8,
        RecordingState::Idle,
        Some(BlockedKind::MicMissing),
        &[]
    )
    .is_empty());
}

#[test]
fn flush_keeps_current_status_and_an_older_retry_bearing_recovery() {
    let queued_recovery = recovery(7, "live", "cloud_failed");
    let mut live = queued_recovery.clone();
    live["expires_in_ms"] = json!(1234);
    let queued = vec![
        ("dictation-recovery", queued_recovery),
        note(8, "mic_silent"),
        (
            "dictation-blocked",
            json!({"generation":8,"kind":"mic_missing"}),
        ),
    ];
    let replay = revalidate(
        queued.clone(),
        8,
        RecordingState::Idle,
        Some(BlockedKind::MicMissing),
        std::slice::from_ref(&live),
    );
    assert_eq!(replay.len(), 3);
    assert_eq!(replay[0], ("dictation-recovery", live));
    assert_eq!(replay[1..], queued[1..]);
}

#[test]
fn flush_drops_live_warning_after_take_ends_but_keeps_it_while_recording() {
    let queued = vec![note(7, "mic_silent_live")];
    for state in [
        RecordingState::Idle,
        RecordingState::Starting,
        RecordingState::Stopping,
        RecordingState::Transcribing,
        RecordingState::Error,
    ] {
        assert!(revalidate(queued.clone(), 7, state, None, &[]).is_empty());
    }
    assert_eq!(
        revalidate(queued.clone(), 7, RecordingState::Recording, None, &[]),
        queued
    );
}

#[test]
fn flush_cannot_replay_a_withdrawn_live_warning() {
    let mut queued = vec![note(7, "mic_silent_live"), note(7, "mic_silent")];
    super::super::withdraw_queued_live_mic_silent(&mut queued, 7, false, &AtomicBool::new(true));
    assert_eq!(
        revalidate(queued, 7, RecordingState::Recording, None, &[]),
        vec![note(7, "mic_silent")],
    );
}

#[test]
fn flush_drops_resolved_or_changed_mic_blockers() {
    let queued = vec![(
        "dictation-blocked",
        json!({"generation":7,"kind":"mic_missing"}),
    )];
    for blocked in [None, Some(BlockedKind::MicBusy)] {
        assert!(revalidate(queued.clone(), 7, RecordingState::Idle, blocked, &[]).is_empty());
    }
}

#[test]
fn flush_drops_recovery_without_a_live_clip_lease() {
    let queued = vec![("dictation-recovery", recovery(7, "gone", "cloud_failed"))];
    assert!(revalidate(queued, 8, RecordingState::Idle, None, &[]).is_empty());
}

#[test]
fn flush_drops_changed_recovery_metadata_instead_of_replaying_old_actions() {
    let queued = recovery(7, "kept", "cloud_failed");
    for (field, value) in [
        ("generation", json!(8)),
        ("kind", json!("integrity")),
        ("engine_short", json!("Network")),
        ("alt_engine_short", Value::Null),
        ("alt_engine_id", Value::Null),
    ] {
        let mut live = queued.clone();
        live[field] = value;
        assert!(
            revalidate(
                vec![("dictation-recovery", queued.clone())],
                8,
                RecordingState::Idle,
                None,
                &[live],
            )
            .is_empty(),
            "{field}"
        );
    }
}

#[test]
fn flush_drops_older_mic_only_recovery_but_not_no_speech_retry() {
    let mic = recovery(7, "mic", "mic_dropped_empty");
    let speech = recovery(7, "speech", "no_speech");
    let replay = revalidate(
        vec![
            ("dictation-recovery", mic.clone()),
            ("dictation-recovery", speech.clone()),
        ],
        8,
        RecordingState::Idle,
        None,
        &[mic.clone(), speech.clone()],
    );
    assert_eq!(replay, vec![("dictation-recovery", speech)]);
    assert_eq!(
        revalidate(
            vec![("dictation-recovery", mic.clone())],
            7,
            RecordingState::Idle,
            None,
            std::slice::from_ref(&mic),
        ),
        vec![("dictation-recovery", mic)],
    );
}

#[test]
fn flush_keeps_only_latest_same_kind_and_same_id_without_losing_other_clips() {
    let old = recovery(6, "a", "cloud_failed");
    let a = recovery(7, "a", "cloud_failed");
    let b = recovery(7, "b", "cloud_failed");
    let first = (
        "dictation-note",
        json!({"generation":7,"kind":"mic_dropped","captured_ms":100}),
    );
    let latest = (
        "dictation-note",
        json!({"generation":7,"kind":"mic_dropped","captured_ms":200}),
    );
    assert_eq!(
        revalidate(
            vec![
                first,
                ("dictation-recovery", old),
                ("dictation-recovery", a.clone()),
                ("dictation-recovery", b.clone()),
                latest.clone(),
            ],
            7,
            RecordingState::Idle,
            None,
            &[a.clone(), b.clone()],
        ),
        vec![("dictation-recovery", a), ("dictation-recovery", b), latest],
    );
}

#[test]
fn late_obsolete_publication_cannot_supersede_current_status() {
    let current = note(8, "mic_silent");
    assert_eq!(
        revalidate(
            vec![current.clone(), note(7, "mic_silent")],
            8,
            RecordingState::Idle,
            None,
            &[],
        ),
        vec![current],
    );
}

#[test]
fn flush_does_not_flash_a_notice_cleared_before_readiness() {
    let clear = ("island-notice-clear", json!({"kind":"long_silence"}));
    assert_eq!(
        revalidate(
            vec![
                ("island-notice", json!({"kind":"long_silence"})),
                clear.clone()
            ],
            7,
            RecordingState::Recording,
            None,
            &[],
        ),
        vec![clear],
    );
}
