use super::*;

#[test]
fn failed_intent_and_unlink_retain_the_owner_and_report_delete_failure() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    let recovery = retain(&mut store, data.path());
    let path = store.clips[0].path.clone();
    let index = path.with_extension("json");
    let saved_index = std::fs::read(&index).unwrap();
    let terminal = store.clips[0].terminal.clone();
    // Deterministic failures also work when tests run with elevated permissions.
    std::fs::remove_file(&index).unwrap();
    std::fs::create_dir(&index).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(!store.discard(&recovery.id));
    assert!(!terminal.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(store.clips.len(), 1);
    assert!(store.clips[0].retire_after_retry);
    store.clear(); // Must return rather than looping forever on the same owner.
    assert_eq!(store.clips.len(), 1);
    std::fs::remove_dir(&index).unwrap();
    std::fs::write(&index, saved_index).unwrap();
    std::fs::remove_dir(&path).unwrap();
    super::super::availability_tests::writer(&path)
        .finalize()
        .unwrap();
    assert!(store.discard(&recovery.id));
    assert!(terminal.load(std::sync::atomic::Ordering::SeqCst));
    let mut restarted = Store::default();
    restarted.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert!(restarted.clips.is_empty());
}

#[test]
fn cap_deletion_failure_leaves_the_incoming_source_in_place() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    for _ in 0..CAP {
        retain(&mut store, data.path());
    }
    let path = store.clips[0].path.clone();
    let index = path.with_extension("json");
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    std::fs::remove_file(&index).unwrap();
    std::fs::create_dir(&index).unwrap();
    let source = data.path().join("incoming.wav");
    super::super::availability_tests::writer(&source)
        .finalize()
        .unwrap();
    assert!(store
        .retain(
            &data.path().join(DIRECTORY),
            &source,
            8,
            RecoveryKind::Integrity
        )
        .is_err());
    assert!(finalized_wav(&source));
    assert_eq!(store.clips.len(), CAP);
    std::fs::remove_dir(&path).unwrap();
    std::fs::remove_dir(&index).unwrap();
    super::super::availability_tests::writer(&path)
        .finalize()
        .unwrap();
    store.clear();
}

#[cfg(unix)]
#[test]
fn unwritable_directory_never_reports_delete_success_or_drops_audio_ownership() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    } // Root bypasses permission checks.
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    let recovery = retain(&mut store, data.path());
    let dir = data.path().join(DIRECTORY);
    let path = store.clips[0].path.clone();
    let permissions = std::fs::metadata(&dir).unwrap().permissions();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let deleted = store.discard(&recovery.id);
    std::fs::set_permissions(&dir, permissions).unwrap();
    assert!(!deleted);
    assert!(finalized_wav(&path));
    assert_eq!(store.clips.len(), 1);
    assert!(store.discard(&recovery.id));
    let mut restarted = Store::default();
    restarted.load_disk(&dir, now_ms());
    assert!(restarted.clips.is_empty());
}

fn retain(store: &mut Store, data: &Path) -> Recovery {
    let source = data.join("capture.wav");
    super::super::availability_tests::writer(&source)
        .finalize()
        .unwrap();
    store
        .retain(&data.join(DIRECTORY), &source, 7, RecoveryKind::CloudFailed)
        .unwrap()
}

#[test]
fn idle_cancel_after_restart_never_discards_restored_failed_takes() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    retain(&mut store, data.path());
    retain(&mut store, data.path());
    store.preserve_on_exit();
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert_eq!(store.clips.len(), 2);
    store.discard_generation(audio::current_recording_generation());
    assert_eq!(store.clips.len(), 2);
    assert!(store.clips.iter().all(|c| c.path.exists()));
    store.clear();
}

#[test]
fn wall_clock_expiry_wins_when_the_monotonic_clock_pauses_during_sleep() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    retain(&mut store, data.path());
    let path = store.clips[0].path.clone();
    // Sleep consumed the wall-clock deadline, but not the monotonic lease.
    store.clips[0].disk.as_mut().unwrap().expires_at_ms = now_ms() - 1;
    assert!(store.clips[0].expires > Instant::now());
    assert!(store.live_recoveries(Instant::now()).is_empty());
    store.expire(Instant::now());
    assert!(!path.exists());
    assert!(store.clips.is_empty());
}

#[test]
fn restart_preserves_audio_and_success_or_discard_deletes_audio_and_index() {
    for success in [true, false] {
        let data = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let recovery = retain(&mut store, data.path());
        let path = store.clips[0].path.clone();
        let index = store.clips[0].disk.as_ref().unwrap().index.clone();
        assert!(index.exists());
        store.clips[0].recovery.retry_failed = true;
        store.clips[0].retry_selection = Some(RetrySelection {
            model: "small.en".into(),
            engine: Some("whisper".into()),
        });
        store.persist(&recovery.id);
        store.preserve_on_exit();
        store.clear();
        assert!(path.exists());
        let mut reloaded = Store::default();
        reloaded.load_disk(&data.path().join(DIRECTORY), now_ms());
        assert_eq!(reloaded.clips.len(), 1);
        assert_eq!(reloaded.clips[0].recovery.id, recovery.id);
        assert!(reloaded.clips[0].recovery.retry_ready);
        assert!(reloaded.clips[0].recovery.retry_failed);
        assert_eq!(
            reloaded.clips[0].retry_selection.as_ref().unwrap().model,
            "small.en"
        );
        // Different model IDs can have the same friendly name.
        assert_eq!(
            alternative_for(
                &reloaded.clips[0],
                &[
                    ("small.en".into(), "whisper".into()),
                    ("small".into(), "whisper".into()),
                ],
                None,
            ),
            Some(("small".into(), "whisper".into()))
        );
        assert_eq!(
            alternative_for(
                &reloaded.clips[0],
                &[("small".into(), "whisper".into())],
                Some("small")
            ),
            Some(("small".into(), "whisper".into())),
            "Alternatives are relative to the failed model, not current Settings"
        );
        {
            let lease = reloaded
                .lease(&recovery.id, false, || {
                    let (cancel, cancelled) = tokio::sync::watch::channel(false);
                    RetryRegistration {
                        generation: 77,
                        cancel,
                        cancelled,
                    }
                })
                .unwrap();
            let settings = crate::commands::settings::Settings {
                current_model: "soniox".into(),
                current_model_engine: "soniox".into(),
                ..Default::default()
            };
            let selected = RetrySelection::resolve(
                &settings,
                lease.kind,
                lease.selection.as_ref(),
                None,
                &[("small.en".into(), "whisper".into())],
                false,
            )
            .unwrap();
            assert_eq!(
                selected.model, "small.en",
                "Home and pill Retry again must retain the model after restart"
            );
            assert_eq!(selected.engine.as_deref(), Some("whisper"));
            reloaded.complete_retry(&recovery.id, false);
        }
        if success {
            reloaded.complete_retry(&recovery.id, false);
            assert!(path.exists());
            reloaded.complete_retry(&recovery.id, true);
        } else {
            reloaded.discard(&recovery.id);
        }
        assert!(!path.exists());
        assert!(!index.exists());
        reloaded.load_disk(&data.path().join(DIRECTORY), now_ms());
        assert!(reloaded.clips.is_empty());
    }
}

#[test]
fn unfinished_writer_is_indexed_only_after_finalization_and_promotion() {
    let data = tempfile::tempdir().unwrap();
    let source = data.path().join("capture.wav");
    let writer = super::super::availability_tests::writer(&source);
    let mut store = Store::default();
    let recovery = store
        .retain(
            &data.path().join(DIRECTORY),
            &source,
            7,
            RecoveryKind::Integrity,
        )
        .unwrap();
    let index = store.clips[0].disk.as_ref().unwrap().index.clone();
    assert!(!index.exists());
    writer.finalize().unwrap();
    let probes = probe_availability(store.availability_paths(None));
    store.apply_availability(probes);
    assert!(
        store
            .poll_finalization(&recovery.id, Instant::now())
            .unwrap()
            .0
    );
    assert!(index.exists());
    store.preserve_on_exit();
    store.clear();
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert_eq!(store.clips.len(), 1);
    store.clear();
}

#[test]
fn failed_index_refresh_at_exit_does_not_delete_previously_persisted_audio() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    retain(&mut store, data.path());
    let path = store.clips[0].path.clone();
    // A directory at the refreshed index target makes replacement fail.
    let blocked = data.path().join(DIRECTORY).join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    store.clips[0].disk.as_mut().unwrap().index = blocked;
    store.preserve_on_exit();
    store.clear();
    assert!(path.exists());
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert_eq!(store.clips.len(), 1);
    store.clear();
}

#[cfg(unix)]
#[test]
fn substituted_directory_never_follows_aliases() {
    let data = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("untouched");
    std::fs::write(&file, b"keep").unwrap();
    std::os::unix::fs::symlink(outside.path(), data.path().join(DIRECTORY)).unwrap();
    let mut store = Store::default();
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert!(store.clips.is_empty());
    assert!(file.exists());
}

#[test]
fn seven_day_expiry_is_not_extended_by_restart() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    retain(&mut store, data.path());
    let deadline = store.clips[0].disk.as_ref().unwrap().expires_at_ms;
    let path = store.clips[0].path.clone();
    store.preserve_on_exit();
    let mut reload = Store::default();
    reload.load_disk(&data.path().join(DIRECTORY), deadline - 1);
    assert_eq!(reload.clips.len(), 1);
    assert_eq!(reload.clips[0].recovery.expires_in_ms, 1);
    // A second simulated process starts at exactly seven days.
    drop(reload);
    let mut expired = Store::default();
    expired.load_disk(&data.path().join(DIRECTORY), deadline);
    assert!(expired.clips.is_empty());
    assert!(!path.exists());
}

#[test]
fn cap_evicts_oldest_and_reset_deletes_durable_audio() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    retain(&mut store, data.path());
    let first = store.clips[0].path.clone();
    for _ in 0..CAP {
        retain(&mut store, data.path());
    }
    assert_eq!(store.clips.len(), CAP);
    assert!(!first.exists());
    store.preserve_on_exit();
    let mut reload = Store::default();
    reload.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert_eq!(reload.clips.len(), CAP);
    let files: Vec<_> = reload.clips.iter().map(|c| c.path.clone()).collect();
    reload.clear();
    assert!(files.iter().all(|p| !p.exists()));
    reload.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert!(reload.clips.is_empty());
}

#[test]
fn crash_between_wav_move_and_index_commit_recovers_without_extending_expiry() {
    for corrupt in [false, true] {
        let data = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let recovery = retain(&mut store, data.path());
        let path = store.clips[0].path.clone();
        let index = path.with_extension("json");
        let modified = now_ms() - 86_400_000;
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + Duration::from_millis(modified))
            .unwrap();
        if corrupt {
            std::fs::write(&index, b"{").unwrap();
        } else {
            std::fs::remove_file(&index).unwrap();
        }
        drop(store); // Simulate process death after the only WAV has moved.
        let mut reload = Store::default();
        reload.load_disk(&data.path().join(DIRECTORY), now_ms());
        assert!(path.exists());
        assert_eq!(reload.clips.len(), 1);
        assert_eq!(reload.clips[0].recovery.id, recovery.id);
        assert_eq!(reload.clips[0].recovery.kind, RecoveryKind::Unknown);
        assert_eq!(
            reload.clips[0].disk.as_ref().unwrap().expires_at_ms,
            modified + TTL_MS
        );
        assert!(serde_json::from_slice::<Entry>(&std::fs::read(index).unwrap()).is_ok());
        reload.preserve_on_exit();
        reload.load_disk(&data.path().join(DIRECTORY), modified + TTL_MS);
        assert!(!path.exists());
    }
}

#[test]
fn recovered_orphans_share_the_twenty_take_cap_and_oldest_first_order() {
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().join(DIRECTORY);
    std::fs::create_dir(&dir).unwrap();
    let now = now_ms();
    let mut paths = Vec::new();
    for age in (0..CAP + 2).rev() {
        let path = dir.join(format!("{}.wav", uuid::Uuid::new_v4()));
        super::super::availability_tests::writer(&path)
            .finalize()
            .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + Duration::from_millis(now - age as u64 * 60_000))
            .unwrap();
        paths.push(path);
    }
    let mut store = Store::default();
    store.load_disk(&dir, now);
    assert_eq!(store.clips.len(), CAP);
    assert!(!paths[0].exists());
    assert!(!paths[1].exists());
    assert!(paths[2..].iter().all(|path| path.exists()));
    store.clear();
}

#[tokio::test]
async fn failed_or_cancelled_busy_retry_survives_cap_eviction() {
    for cancel in [false, true] {
        let data = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let first = retain(&mut store, data.path());
        let original = store.clips[0].path.clone();
        let lease = store
            .lease(&first.id, false, || {
                let (cancel, cancelled) = tokio::sync::watch::channel(false);
                RetryRegistration {
                    generation: 77,
                    cancel,
                    cancelled,
                }
            })
            .unwrap();
        retain(&mut store, data.path());
        let oldest_idle = store.clips[1].path.clone();
        for _ in 1..CAP {
            retain(&mut store, data.path());
        }
        assert_eq!(store.clips.len(), CAP);
        assert!(!oldest_idle.exists());
        assert!(original.exists());
        if cancel {
            store.cancel_retry();
        }
        let outcome: Result<(), RetryError> =
            cancellable_retry(lease.cancelled.clone(), async { Err("Failed".into()) }).await;
        assert_eq!(
            outcome,
            Err(if cancel {
                RetryError::Cancelled
            } else {
                RetryError::Failed
            })
        );
        store.complete_retry(&first.id, false);
        assert!(original.exists());
        assert!(original.with_extension("json").exists());
        assert_eq!(store.clips.len(), CAP);
        assert!(!store.clips[0].busy);
        drop(lease);
        store.clear();
    }
}

#[test]
fn remote_failed_retry_is_still_labelled_network_after_restart() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    let recovery = retain(&mut store, data.path());
    store.clips[0].recovery.kind = RecoveryKind::RemoteOffline;
    store.clips[0].recovery.retry_failed = true;
    // Older metadata may have recorded the unrelated local Settings model.
    store.clips[0].recovery.engine_short = "Soniox".into();
    store.clips[0].retry_selection = Some(RetrySelection {
        model: "soniox".into(),
        engine: None,
    });
    store.persist(&recovery.id);
    store.preserve_on_exit();
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    let value = serde_json::to_value(&store.clips[0].recovery).unwrap();
    assert_eq!(value["engine_short"], "Your other computer");
    assert_eq!(value["retry_failed"], true);
    let candidates = vec![("small.en".into(), "whisper".into())];
    assert_eq!(
        alternative_for(&store.clips[0], &candidates, Some("small.en")),
        Some(candidates[0].clone())
    );
    store.clear();
}

#[test]
fn crash_during_deletion_never_resurrects_retired_audio() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    let recovery = retain(&mut store, data.path());
    let path = store.clips[0].path.clone();
    let index = path.with_extension("json");
    // discard() and complete_retry(true) both take ownership before unlinking.
    let _retired = store.take(&recovery.id).unwrap();
    assert!(path.exists());
    assert_eq!(
        serde_json::from_slice::<Entry>(&std::fs::read(&index).unwrap())
            .unwrap()
            .expires_at_ms,
        0
    );
    // Simulate an unlink error, followed by process death.
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert!(store.clips.is_empty());
    assert!(
        index.exists(),
        "Deletion intent must survive an unlink error"
    );
    std::fs::remove_dir(&path).unwrap();
    super::super::availability_tests::writer(&path)
        .finalize()
        .unwrap();
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert!(store.clips.is_empty());
    assert!(!path.exists());
    assert!(!index.exists());
}

#[test]
fn temporarily_unreadable_or_unfinalized_wav_keeps_retry_metadata() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    let recovery = retain(&mut store, data.path());
    let path = store.clips[0].path.clone();
    let index = path.with_extension("json");
    store.clips[0].retry_selection = Some(RetrySelection {
        model: "small.en".into(),
        engine: Some("whisper".into()),
    });
    store.preserve_on_exit();
    let before = std::fs::read(&index).unwrap();
    let pending = super::super::availability_tests::writer(&path);
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert!(store.clips.is_empty());
    assert_eq!(std::fs::read(&index).unwrap(), before);
    pending.finalize().unwrap();
    store.load_disk(&data.path().join(DIRECTORY), now_ms());
    assert_eq!(store.clips[0].recovery.id, recovery.id);
    assert_eq!(
        store.clips[0].retry_selection.as_ref().unwrap().model,
        "small.en"
    );
    store.clear();
}

#[test]
fn discard_and_success_preserve_deletion_intent_when_wav_unlink_fails() {
    for success in [false, true] {
        let data = tempfile::tempdir().unwrap();
        let mut store = Store::default();
        let recovery = retain(&mut store, data.path());
        let path = store.clips[0].path.clone();
        let index = path.with_extension("json");
        // A directory at the owned WAV name deterministically fails remove_file.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        if success {
            store.complete_retry(&recovery.id, true);
        } else {
            store.discard(&recovery.id);
        }
        assert!(store.clips.is_empty());
        assert_eq!(
            serde_json::from_slice::<Entry>(&std::fs::read(&index).unwrap())
                .unwrap()
                .expires_at_ms,
            0
        );
        std::fs::remove_dir(&path).unwrap();
        super::super::availability_tests::writer(&path)
            .finalize()
            .unwrap();
        store.load_disk(&data.path().join(DIRECTORY), now_ms());
        assert!(
            store.clips.is_empty(),
            "Discarded or successfully retried audio must never return"
        );
        assert!(!path.exists());
        assert!(!index.exists());
    }
}

#[test]
fn recovered_unknown_resolution_passes_the_closed_telemetry_validator() {
    let data = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    retain(&mut store, data.path());
    store.clips[0].recovery.kind = RecoveryKind::Unknown;
    store.clips[0].recovery.generation = u64::MAX;
    store.clips[0].trace = Some(crate::observability::Trace {
        generation: u64::MAX,
        id: uuid::Uuid::new_v4().to_string(),
        properties: vec![],
    });
    crate::product_analytics::take_test_captures();
    resolve_clip(&store.clips[0], "discarded");
    let events = crate::product_analytics::take_test_captures();
    assert!(events.iter().any(|event| event.0 == "recovery_resolved"));
    store.clear();
}
