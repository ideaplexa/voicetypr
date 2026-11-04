use super::*;

#[test]
fn native_timer_callback_runs_without_status_or_item_locks() {
    let status = Mutex::new(Status {
        started: Some(Instant::now()),
        revision: 7,
        ..Default::default()
    });
    let item = Mutex::new(Some(42));
    let mut called = false;
    assert!(update_recording_label(&status, &item, 7, |handle, text| {
        assert_eq!(handle, 42);
        assert!(text.contains("Recording"));
        let mut status = status
            .try_lock()
            .expect("native callback still holds STATUS");
        let mut item = item
            .try_lock()
            .expect("native callback still holds STATUS_ITEM");
        status.revision += 1;
        *item = Some(43);
        called = true;
    }));
    assert!(called);
}

#[test]
fn stale_or_stopped_timer_does_not_call_native_menu() {
    let status = Mutex::new(Status {
        started: Some(Instant::now()),
        revision: 7,
        ..Default::default()
    });
    let item = Mutex::new(Some(42));
    assert!(!update_recording_label(&status, &item, 6, |_, _| panic!(
        "stale timer"
    )));
    status.lock().unwrap().started = None;
    assert!(!update_recording_label(&status, &item, 7, |_, _| panic!(
        "stopped timer"
    )));
}

#[test]
fn readiness_runs_contended_drain_on_a_blocking_worker() {
    let source = include_str!("../../commands/pill_feedback.rs");
    assert!(source.contains("pub async fn pill_feedback_ready("));
    assert!(source.contains("spawn_blocking(move || drain_feedback(app))"));
}
