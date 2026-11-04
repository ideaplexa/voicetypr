use super::*;
use crate::{recording::island::BlockedKind, RecordingState};

fn validate(
    outbox: &mut Outbox,
    owned: &AtomicBool,
    state: RecordingState,
    blocked: Option<BlockedKind>,
    live: &[Value],
) -> Option<Vec<Event>> {
    outbox.rebuild_from_live(live);
    let batch = replay::revalidate(outbox.take_batch(owned)?, 7, state, blocked, live);
    outbox.validated(&batch, owned);
    Some(batch)
}

fn kept() -> Value {
    json!({"id":"kept","generation":6,"kind":"cloud_failed","expires_in_ms":1000})
}

#[test]
fn captured_recovery_is_rebuilt_from_the_current_lease_in_the_new_epoch() {
    let owned = AtomicBool::new(true);
    let mut outbox = Outbox::new();
    outbox.enqueue("dictation-recovery", kept());
    assert!(outbox.begin_flush());
    let captured = validate(&mut outbox, &owned, RecordingState::Idle, None, &[kept()]).unwrap();
    assert_eq!(captured[0].0, "dictation-recovery");
    let old_epoch = outbox.epoch();
    outbox.close(&owned);
    assert!(!owned.load(Ordering::SeqCst));
    assert!(!outbox.accepts_visibility(old_epoch));
    // The captured emitter remains the sole drainer, but no longer owns the UI.
    assert!(!outbox.begin_flush());
    assert_ne!(outbox.epoch(), old_epoch);
    let mut current = kept();
    current["expires_in_ms"] = 500.into();
    current["alt_engine_id"] = "new-alternative".into();
    let replacement = validate(
        &mut outbox,
        &owned,
        RecordingState::Idle,
        None,
        std::slice::from_ref(&current),
    )
    .unwrap();
    assert_eq!(replacement, vec![("dictation-recovery", current.clone())]);
    assert_eq!(outbox.tag(current)["renderer_epoch"], old_epoch + 1);
    assert!(outbox.accepts_visibility(old_epoch + 1));
    assert!(owned.load(Ordering::SeqCst));
    assert!(validate(&mut outbox, &owned, RecordingState::Idle, None, &[]).is_none());
    assert!(outbox.ready);
}

#[test]
fn consumed_recovery_is_not_restored_when_the_live_store_omits_it() {
    // Discard/retry success/expiry/busy/shutdown all omit the lease from the
    // authoritative snapshot; old pending or consumed metadata is not a source.
    for finish_old_drain in [false, true] {
        let owned = AtomicBool::new(true);
        let mut outbox = Outbox::new();
        outbox.enqueue("dictation-recovery", kept());
        assert!(outbox.begin_flush());
        validate(&mut outbox, &owned, RecordingState::Idle, None, &[kept()]).unwrap();
        if finish_old_drain {
            assert!(validate(&mut outbox, &owned, RecordingState::Idle, None, &[]).is_none());
        }
        outbox.close(&owned);
        assert_eq!(outbox.begin_flush(), finish_old_drain);
        assert!(validate(&mut outbox, &owned, RecordingState::Idle, None, &[]).is_none());
        assert!(!owned.load(Ordering::SeqCst));
        assert!(outbox.ready);
    }
}

#[test]
fn direct_current_status_is_retained_but_delayed_older_publication_cannot_replace_it() {
    let owned = AtomicBool::new(false);
    let mut outbox = Outbox::new();
    assert!(outbox.begin_flush());
    assert!(outbox.take_batch(&owned).is_none());
    let current = json!({"generation":7,"kind":"mic_dropped","captured_ms":2000});
    outbox.remember("dictation-note", &current);
    outbox.remember(
        "dictation-note",
        &json!({"generation":6,"kind":"mic_dropped","captured_ms":1000}),
    );
    outbox.remember(
        "dictation-blocked",
        &json!({"generation":7,"kind":"mic_busy","action":"choose_mic"}),
    );
    outbox.close(&owned);
    assert!(outbox.begin_flush());
    let rebuilt = validate(&mut outbox, &owned, RecordingState::Idle, None, &[]).unwrap();
    assert_eq!(rebuilt, vec![("dictation-note", current)]);
    assert!(owned.load(Ordering::SeqCst));
}

#[test]
fn replacement_keeps_only_the_still_live_current_blocker() {
    let owned = AtomicBool::new(true);
    let mut outbox = Outbox::new();
    let blocker = json!({"generation":7,"kind":"mic_busy","action":"choose_mic"});
    outbox.remember("dictation-blocked", &blocker);
    outbox.remember(
        "dictation-blocked",
        &json!({"generation":6,"kind":"mic_missing","action":"choose_mic"}),
    );
    outbox.close(&owned);
    assert!(outbox.begin_flush());
    let blocked = BlockedKind::MicBusy;
    let rebuilt = validate(
        &mut outbox,
        &owned,
        RecordingState::Idle,
        Some(blocked),
        &[],
    )
    .unwrap();
    assert_eq!(rebuilt, vec![("dictation-blocked", blocker)]);
}

#[test]
fn old_readiness_worker_cannot_consume_the_replacement_epoch() {
    let owned = AtomicBool::new(true);
    let mut outbox = Outbox::new();
    let old_epoch = outbox.epoch();
    outbox.close(&owned);
    assert!(!outbox.begin_flush_for(old_epoch));
    assert!(!outbox.requested);
    assert!(!outbox.ready);
    assert!(validate(&mut outbox, &owned, RecordingState::Idle, None, &[kept()]).is_none());
    assert!(!owned.load(Ordering::SeqCst));
    assert!(outbox.begin_flush_for(outbox.epoch()));
    let rebuilt = validate(&mut outbox, &owned, RecordingState::Idle, None, &[kept()]).unwrap();
    assert_eq!(rebuilt, vec![("dictation-recovery", kept())]);
}

#[test]
fn withdrawn_or_no_longer_recording_warning_cannot_be_rehydrated() {
    for withdraw in [false, true] {
        let owned = AtomicBool::new(true);
        let mut outbox = Outbox::new();
        outbox.remember(
            "dictation-note",
            &json!({"generation":7,"kind":"mic_silent_live"}),
        );
        outbox.close(&owned);
        if withdraw {
            outbox.withdraw(7, &owned);
        }
        assert!(outbox.begin_flush());
        let state = if withdraw {
            RecordingState::Recording
        } else {
            RecordingState::Idle
        };
        let batch = validate(&mut outbox, &owned, state, None, &[]).unwrap();
        assert!(batch.iter().all(|(event, _)| !replay::produces_ui(event)));
        assert!(validate(&mut outbox, &owned, state, None, &[]).is_none());
        assert!(!owned.load(Ordering::SeqCst));
    }
}
