use super::*;
use std::sync::{atomic::Ordering, Arc, Mutex};

fn warning() -> Event {
    (
        "dictation-note",
        json!({"generation":7,"kind":"mic_silent_live"}),
    )
}

fn validate(outbox: &mut Outbox, owned: &AtomicBool, live: &[Value]) -> Option<Vec<Event>> {
    let batch = outbox.take_batch(owned)?;
    let batch = replay::revalidate(batch, 7, crate::RecordingState::Recording, None, live);
    outbox.validated(&batch);
    Some(batch)
}

#[test]
fn deletion_after_batch_capture_is_emitted_after_recovery_not_before_it() {
    // Discard, successful retry and expiry all use Outbox::retire.
    for _deletion in ["discard", "retry_success", "expiry"] {
        let owned = AtomicBool::new(true);
        let mut outbox = Outbox::new();
        let live = json!({"id":"kept","generation":6,"kind":"cloud_failed","expires_in_ms":1000});
        outbox.enqueue("dictation-recovery", live.clone());
        assert!(outbox.begin_flush());
        let mut emitted = validate(&mut outbox, &owned, &[live]).unwrap();
        assert!(!outbox.ready);
        // The clip disappears while this batch is outside the queue lock.
        outbox.retire("kept", &owned);
        assert!(
            !outbox.begin_flush(),
            "duplicate readiness must not emit a second batch"
        );
        assert!(!outbox.ready);
        emitted.extend(validate(&mut outbox, &owned, &[]).unwrap());
        assert!(validate(&mut outbox, &owned, &[]).is_none());
        assert!(outbox.ready);
        assert_eq!(
            emitted.iter().map(|e| e.0).collect::<Vec<_>>(),
            ["dictation-recovery", "kept-dictation-expired"]
        );
    }
}

#[test]
fn withdrawal_during_emit_follows_the_captured_warning() {
    let owned = AtomicBool::new(true);
    let mut outbox = Outbox::new();
    outbox.enqueue(warning().0, warning().1);
    assert!(outbox.begin_flush());
    let mut emitted = validate(&mut outbox, &owned, &[]).unwrap();
    outbox.withdraw(7, &owned);
    assert!(
        owned.load(Ordering::SeqCst),
        "in-flight UI still owns the panel"
    );
    assert!(!outbox.ready);
    emitted.extend(validate(&mut outbox, &owned, &[]).unwrap());
    assert!(validate(&mut outbox, &owned, &[]).is_none());
    assert_eq!(
        emitted.iter().map(|e| e.0).collect::<Vec<_>>(),
        ["dictation-note", "dictation-note-withdraw"]
    );
}

#[test]
fn newer_publication_during_emit_cannot_be_overtaken_by_captured_replay() {
    let owned = AtomicBool::new(true);
    let mut outbox = Outbox::new();
    let old = json!({"generation":7,"kind":"mic_dropped","captured_ms":1000});
    let new = json!({"generation":7,"kind":"mic_dropped","captured_ms":2000});
    outbox.enqueue("dictation-note", old.clone());
    assert!(outbox.begin_flush());
    let mut emitted = validate(&mut outbox, &owned, &[]).unwrap();
    outbox.enqueue("dictation-note", new.clone());
    assert!(!outbox.begin_flush());
    assert!(!outbox.ready);
    emitted.extend(validate(&mut outbox, &owned, &[]).unwrap());
    assert!(validate(&mut outbox, &owned, &[]).is_none());
    assert_eq!(
        emitted,
        vec![("dictation-note", old), ("dictation-note", new)]
    );
}

#[test]
fn clear_racing_a_batch_is_ordered_but_clear_only_startup_releases_ownership() {
    let owned = AtomicBool::new(true);
    let mut outbox = Outbox::new();
    outbox.enqueue("island-notice", json!({"kind":"long_silence"}));
    outbox.enqueue("island-notice-clear", json!({"kind":"long_silence"}));
    assert!(outbox.begin_flush());
    let batch = validate(&mut outbox, &owned, &[]).unwrap();
    assert_eq!(batch[0].0, "island-notice-clear");
    assert!(validate(&mut outbox, &owned, &[]).is_none());
    assert!(!owned.load(Ordering::SeqCst));

    let mut outbox = Outbox::new();
    owned.store(true, Ordering::SeqCst);
    outbox.enqueue("island-notice", json!({"kind":"long_silence"}));
    assert!(outbox.begin_flush());
    let first = validate(&mut outbox, &owned, &[]).unwrap();
    outbox.enqueue("island-notice-clear", json!({"kind":"long_silence"}));
    let second = validate(&mut outbox, &owned, &[]).unwrap();
    assert!(validate(&mut outbox, &owned, &[]).is_none());
    assert_eq!(
        (first[0].0, second[0].0),
        ("island-notice", "island-notice-clear")
    );
    // Once UI has actually been emitted, the renderer releases its ownership.
    assert!(owned.load(Ordering::SeqCst));
}

#[test]
fn active_flush_never_evicts_removal_of_an_in_flight_card() {
    let owned = AtomicBool::new(true);
    let mut outbox = Outbox::new();
    outbox.enqueue(warning().0, warning().1);
    assert!(outbox.begin_flush());
    validate(&mut outbox, &owned, &[]).unwrap();
    outbox.withdraw(7, &owned);
    for index in 0..64 {
        outbox.enqueue("island-notice", json!({"kind":format!("notice-{index}")}));
    }
    let next = validate(&mut outbox, &owned, &[]).unwrap();
    assert!(next
        .iter()
        .any(|(event, _)| *event == "dictation-note-withdraw"));
    assert!(!outbox.ready);
    assert!(validate(&mut outbox, &owned, &[]).is_none());
}

#[test]
fn simultaneous_ready_calls_have_exactly_one_drainer() {
    let outbox = Arc::new(Mutex::new(Outbox::new()));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let outbox = outbox.clone();
            std::thread::spawn(move || outbox.lock().unwrap().begin_flush())
        })
        .collect();
    let winners = threads
        .into_iter()
        .map(|t| usize::from(t.join().unwrap()))
        .sum::<usize>();
    assert_eq!(winners, 1);
    let mut outbox = outbox.lock().unwrap();
    assert!(!outbox.ready);
    assert!(outbox.take_batch(&AtomicBool::new(false)).is_none());
    assert!(outbox.ready);
    assert!(
        !outbox.begin_flush(),
        "already-ready calls must not touch renderer ownership"
    );
}

#[test]
fn close_during_emit_cannot_install_a_second_flusher() {
    let owned = AtomicBool::new(false);
    let mut outbox = Outbox::new();
    outbox.enqueue(warning().0, warning().1);
    assert!(outbox.begin_flush());
    validate(&mut outbox, &owned, &[]).unwrap();
    outbox.close();
    assert!(
        !outbox.begin_flush(),
        "the original emitter still owns its batch"
    );
    outbox.enqueue("island-notice-clear", json!({"kind":"long_silence"}));
    validate(&mut outbox, &owned, &[]).unwrap();
    assert!(validate(&mut outbox, &owned, &[]).is_none());
    assert!(outbox.ready);
}

#[test]
fn unready_removals_are_scoped_ordered_and_survive_generation_revalidation() {
    let owned = AtomicBool::new(true);
    let mut outbox = Outbox::new();
    outbox.enqueue(
        "dictation-note",
        json!({"generation":6,"kind":"mic_silent_live"}),
    );
    outbox.enqueue(warning().0, warning().1);
    outbox.withdraw(6, &owned);
    outbox.retire("gone", &owned);
    assert!(outbox.begin_flush());
    let batch = validate(&mut outbox, &owned, &[]).unwrap();
    assert_eq!(
        batch.iter().map(|e| e.0).collect::<Vec<_>>(),
        [
            "dictation-note",
            "dictation-note-withdraw",
            "kept-dictation-expired"
        ]
    );
    assert_eq!(batch[0].1["generation"], 7);
}
