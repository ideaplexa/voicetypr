use super::*;

#[test]
fn native_timer_callback_runs_without_status_or_item_locks() {
    let status = Mutex::new(Status {
        started: Some(Instant::now()),
        revision: 7,
        ..Default::default()
    });
    let handle = Arc::new(42);
    let item = Mutex::new(Some(handle.clone()));
    let mut called = false;
    assert!(update_recording_label(
        &status,
        &item,
        7,
        Some(handle),
        |handle, text| {
            assert_eq!(*handle, 42);
            assert!(text.contains("Recording"));
            let mut status = status
                .try_lock()
                .expect("native callback still holds STATUS");
            let mut item = item
                .try_lock()
                .expect("native callback still holds STATUS_ITEM");
            status.revision += 1;
            *item = Some(Arc::new(43));
            called = true;
        }
    ));
    assert!(called);
}

#[test]
fn stale_or_stopped_timer_does_not_call_native_menu() {
    let status = Mutex::new(Status {
        started: Some(Instant::now()),
        revision: 7,
        ..Default::default()
    });
    let handle = Arc::new(42);
    let item = Mutex::new(Some(handle.clone()));
    assert!(!update_recording_label(
        &status,
        &item,
        6,
        Some(handle.clone()),
        |_, _| panic!("stale timer")
    ));
    status.lock().unwrap().started = None;
    assert!(!update_recording_label(
        &status,
        &item,
        7,
        Some(handle),
        |_, _| panic!("stopped timer")
    ));
}

#[test]
fn stop_and_ready_menu_install_before_main_dispatch_drop_the_old_recording_write() {
    let status = Mutex::new(Status {
        started: Some(Instant::now()),
        revision: 7,
        ..Default::default()
    });
    let item = Mutex::new(Some(Arc::new(42)));
    let revision = status.lock().unwrap().revision; // timer observes Recording
    {
        let mut status = status.lock().unwrap();
        status.started = None;
        status.revision += 1;
    }
    *item.lock().unwrap() = Some(Arc::new(43)); // stop installs Ready
    let captured = item.lock().unwrap().clone(); // worker sees the newer item
                                                 // This is the same helper run by the production main-thread closure.
    assert!(!update_recording_label(
        &status,
        &item,
        revision,
        captured,
        |_, _| panic!("old timer must not overwrite the Ready item")
    ));
}

#[test]
fn replacement_after_capture_is_rejected_even_with_the_same_revision_and_item_value() {
    let status = Mutex::new(Status {
        started: Some(Instant::now()),
        revision: 7,
        ..Default::default()
    });
    let captured = Arc::new(42);
    let item = Mutex::new(Some(captured.clone()));
    *item.lock().unwrap() = Some(Arc::new(42)); // a distinct native menu install
    assert!(!update_recording_label(
        &status,
        &item,
        7,
        Some(captured),
        |_, _| panic!("queued write targets a retired menu instance")
    ));
}

#[test]
fn readiness_runs_contended_drain_on_a_blocking_worker() {
    let source = include_str!("../../commands/pill_feedback.rs");
    assert!(source.contains("pub async fn pill_feedback_ready("));
    assert!(source.contains("spawn_blocking(move ||"));
    assert!(source.contains("drain_feedback(app, epoch)"));
}
