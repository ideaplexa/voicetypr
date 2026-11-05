use super::*;

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
        assert!(
            alternative_for(
                &reloaded.clips[0],
                &[("small".into(), "whisper".into())],
                Some("small"),
            )
            .is_none(),
            "Another model must differ from Retry again's current model"
        );
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
