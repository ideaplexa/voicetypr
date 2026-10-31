use super::*;

#[test]
fn saving_never_yields_between_copy_and_late_cancel_revocation() {
    let audio = include_str!("../commands/audio.rs");
    let save = audio
        .split_once("async fn save_recording_internal(")
        .unwrap()
        .1
        .split_once("/// Save recording without cleanup")
        .unwrap()
        .0;
    let after_copy = save.split_once("let copy_result =").unwrap().1;
    assert!(
        !after_copy.contains(".await"),
        "aborting at an await after the copy would bypass late-cancel revocation"
    );
    assert!(after_copy.contains("storage::schedule_maintenance(app.clone());"));
}

#[test]
fn only_potentially_live_workers_defer_deletion_after_stop_errors() {
    assert!(writer_may_be_running(
        "Recording thread failed to stop within timeout"
    ));
    assert!(writer_may_be_running(
        "Writer thread failed to finalize within timeout"
    ));
    assert!(writer_may_be_running("Recording thread panicked"));
    assert!(!writer_may_be_running("Writer thread panicked"));
    assert!(!writer_may_be_running("WAV finalize failed: disk error"));
    assert!(!writer_may_be_running("Audio device error: disconnected"));
}

fn write_aged(path: &Path, now: SystemTime, age: Duration) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"audio").unwrap();
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(now - age))
        .unwrap();
}
fn capture_path(data: &Path, filename: &str) -> PathBuf {
    data.join("captures")
        .join(uuid::Uuid::new_v4().to_string())
        .join(filename)
}
async fn wait_removed(path: &Path) {
    tokio::time::timeout(Duration::from_secs(15), async {
        while path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("background capture cleanup should finish");
}
#[tokio::test]
async fn terminal_take_owner_removes_raw_and_prepared_audio_without_an_opted_in_save() {
    let data = tempfile::tempdir().unwrap();
    let files = CaptureFiles::create(data.path()).unwrap();
    let raw = files.audio_path();
    std::fs::write(&raw, b"capture").unwrap();
    let normalized = raw.parent().unwrap().join("normalized_20260101_120000.wav");
    std::fs::write(&normalized, b"prepared").unwrap();
    files.writer_stopped(false);
    drop(files);
    wait_removed(raw.parent().unwrap()).await;
    assert!(!raw.exists());
    assert!(!normalized.exists());
    assert!(!data.path().join("recordings").exists());
}
#[tokio::test]
async fn cancelling_transcription_task_releases_capture_audio_without_saving() {
    let data = tempfile::tempdir().unwrap();
    let files = CaptureFiles::create(data.path()).unwrap();
    let raw = files.audio_path();
    std::fs::write(&raw, b"capture").unwrap();
    files.writer_stopped(false);
    let (ready, started) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _files = files;
        ready.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    started.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    wait_removed(raw.parent().unwrap()).await;
    assert!(!raw.exists());
}
#[tokio::test]
async fn failed_preparation_releases_raw_and_partial_normalization_without_saving() {
    let data = tempfile::tempdir().unwrap();
    let files = CaptureFiles::create(data.path()).unwrap();
    let raw = files.audio_path();
    std::fs::write(&raw, b"capture").unwrap();
    files.writer_stopped(false);
    let partial = raw.parent().unwrap().join("normalized_20260101_120000.wav");
    let result = (|_files: CaptureFiles| -> Result<(), ()> {
        std::fs::write(&partial, b"partial").unwrap();
        Err(())?;
        Ok(())
    })(files);
    assert!(result.is_err());
    wait_removed(raw.parent().unwrap()).await;
    assert!(!partial.exists());
}

#[test]
fn sweep_removes_only_old_orphans_and_keeps_fresh_active_and_unrelated_files() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let old = ORPHAN_AGE + Duration::from_secs(1);
    let orphan = capture_path(data.path(), "recording.wav");
    let fresh = capture_path(data.path(), "recording.wav");
    let active = capture_path(data.path(), "recording.wav");
    let unrelated = orphan.parent().unwrap().join("notes.txt");
    let unrelated_wav = orphan.parent().unwrap().join("recording_personal.wav");
    let legacy = data.path().join("recordings/recording_123.wav");
    let legacy_normalized = data
        .path()
        .join("recordings/normalized_20260101_120001.wav");
    for path in [
        &orphan,
        &active,
        &unrelated,
        &unrelated_wav,
        &legacy,
        &legacy_normalized,
    ] {
        write_aged(path, now, old);
    }
    write_aged(&fresh, now, Duration::from_secs(599));
    let counts = sweep(
        data.path(),
        None,
        Some(&HashSet::new()),
        &HashSet::new(),
        &HashSet::from([active.parent().unwrap().to_owned()]),
        None,
        now,
    );
    assert_eq!(counts.orphans, 3);
    for removed in [&orphan, &legacy, &legacy_normalized] {
        assert!(!removed.exists());
    }
    for kept in [&fresh, &active, &unrelated, &unrelated_wav] {
        assert!(kept.exists());
    }
}

#[test]
fn saving_on_history_and_unreferenced_saved_recordings_survive_orphan_sweep() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let names = ["2026-01-01_12-00-00_12345678.wav", "recording_456.wav"];
    let unreferenced_save = data
        .path()
        .join("recordings/2026-01-01_12-00-00_87654321.wav");
    for path in names
        .iter()
        .map(|n| data.path().join("recordings").join(n))
        .chain([unreferenced_save.clone()])
    {
        write_aged(&path, now, Duration::from_secs(86400));
    }
    let refs = names.into_iter().map(str::to_owned).collect();
    let counts = sweep(
        data.path(),
        None,
        Some(&refs),
        &HashSet::new(),
        &HashSet::new(),
        None,
        now,
    );
    assert_eq!(counts.orphans, 0);
    assert_eq!(counts.expired, 0);
    assert!(
        unreferenced_save.exists(),
        "clearing History is not consent to delete saved audio"
    );
    for name in names {
        assert!(data.path().join("recordings").join(name).exists());
    }
}

#[test]
fn unreadable_history_keeps_legacy_captures_conservatively() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let legacy = data.path().join("recordings/recording_123.wav");
    write_aged(&legacy, now, Duration::from_secs(86400 * 40));
    sweep(
        data.path(),
        None,
        None,
        &HashSet::new(),
        &HashSet::new(),
        Some(30),
        now,
    );
    assert!(legacy.exists());
}

#[test]
fn long_running_file_uploads_are_not_mistaken_for_orphan_captures() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let upload = data
        .path()
        .join("recordings")
        .join(format!("upload-normalized-{}.wav", uuid::Uuid::new_v4()));
    write_aged(&upload, now, Duration::from_secs(86400 * 40));
    let counts = sweep(
        data.path(),
        None,
        Some(&HashSet::new()),
        &HashSet::new(),
        &HashSet::new(),
        Some(7),
        now,
    );
    assert_eq!(counts.orphans, 0);
    assert_eq!(counts.expired, 0);
    assert!(
        upload.exists(),
        "file-job ownership can outlive the capture orphan grace"
    );
    let audio = include_str!("../commands/audio.rs");
    let normalize = audio
        .split_once("async fn normalize_upload_audio_for_cloud(")
        .unwrap()
        .1
        .split_once("fn cloud_provider_supports_diarized_words")
        .unwrap()
        .0;
    assert!(normalize.contains("upload-normalized-"));
    assert!(!normalize.contains("format!(\"normalized_"));
}

#[test]
fn startup_sweep_applies_configured_retention_including_history_references() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let expired = data
        .path()
        .join("recordings/2026-01-01_12-00-00_12345678.wav");
    let recent = data
        .path()
        .join("recordings/2026-01-02_12-00-00_12345678.wav");
    let unrelated = data.path().join("recordings/personal.wav");
    write_aged(&unrelated, now, Duration::from_secs(86400 * 40));
    write_aged(&expired, now, Duration::from_secs(86400 * 8));
    write_aged(&recent, now, Duration::from_secs(86400 * 6));
    let refs = HashSet::from([expired.file_name().unwrap().to_str().unwrap().to_owned()]);
    let keep_forever = sweep(
        data.path(),
        None,
        Some(&refs),
        &HashSet::new(),
        &HashSet::new(),
        None,
        now,
    );
    assert_eq!(keep_forever.expired, 0);
    assert!(expired.exists());
    let bounded = sweep(
        data.path(),
        None,
        Some(&refs),
        &HashSet::new(),
        &HashSet::new(),
        Some(7),
        now,
    );
    assert_eq!(bounded.expired, 1);
    assert!(!expired.exists());
    assert!(recent.exists());
    assert!(unrelated.exists());
    assert!(include_str!("../lib.rs").contains("storage::start_maintenance(app.handle().clone())"));
}

#[test]
fn recovery_sweep_preserves_kept_clips_and_retry_leases_but_removes_crash_orphans() {
    let data = tempfile::tempdir().unwrap();
    let legacy = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let stale = data
        .path()
        .join("recovery")
        .join(format!("{}.wav", uuid::Uuid::new_v4()));
    let kept = data
        .path()
        .join("recovery")
        .join(format!("{}.wav", uuid::Uuid::new_v4()));
    let lease = data.path().join("recovery/.tmplease.wav");
    let crashed_legacy = legacy.path().join(format!("{}.wav", uuid::Uuid::new_v4()));
    let unrelated = data.path().join("recovery/personal.wav");
    for p in [&stale, &kept, &lease, &crashed_legacy, &unrelated] {
        write_aged(p, now, ORPHAN_AGE + Duration::from_secs(1));
    }
    let counts = sweep(
        data.path(),
        Some(legacy.path()),
        Some(&HashSet::new()),
        &HashSet::from([kept.clone(), lease.clone()]),
        &HashSet::new(),
        None,
        now,
    );
    assert_eq!(counts.orphans, 2);
    assert!(!stale.exists());
    assert!(!crashed_legacy.exists());
    assert!(kept.exists());
    assert!(lease.exists());
    assert!(unrelated.exists());
}

#[tokio::test]
async fn detached_writer_is_protected_until_header_is_finalized() {
    let data = tempfile::tempdir().unwrap();
    let files = CaptureFiles::create(data.path()).unwrap();
    let path = files.audio_path();
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    writer.write_sample(1_i16).unwrap();
    files.writer_stopped(true);
    drop(files);
    {
        let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        sweep(
            data.path(),
            None,
            Some(&HashSet::new()),
            &HashSet::new(),
            &active.keys().cloned().collect(),
            None,
            SystemTime::now() + Duration::from_secs(86400),
        );
    }
    assert!(path.exists());
    writer.finalize().unwrap();
    wait_removed(path.parent().unwrap()).await;
}

#[tokio::test]
async fn reset_clears_saved_capture_and_recovery_storage_but_not_active_writer() {
    let data = tempfile::tempdir().unwrap();
    let legacy = tempfile::tempdir().unwrap();
    let files = CaptureFiles::create(data.path()).unwrap();
    let active = files.audio_path();
    let abandoned = capture_path(data.path(), "recording.wav");
    let saved = data.path().join("recordings/saved.wav");
    let recovery = data.path().join("recovery/kept.wav");
    let old_recovery = legacy.path().join("kept.wav");
    for p in [&active, &abandoned, &saved, &recovery, &old_recovery] {
        write_aged(p, SystemTime::now(), Duration::ZERO);
    }
    clear_storage(data.path(), Some(legacy.path().to_owned())).unwrap();
    assert!(active.exists());
    for p in [&abandoned, &saved, &recovery, &old_recovery] {
        assert!(!p.exists());
    }
    files.writer_stopped(false);
    drop(files);
    wait_removed(active.parent().unwrap()).await;
}

#[cfg(unix)]
#[test]
fn sweep_does_not_follow_symlinks_outside_capture_storage() {
    let data = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let source = external.path().join("recording.wav");
    let now = SystemTime::now();
    write_aged(&source, now, Duration::from_secs(86400));
    let link = data
        .path()
        .join("captures")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(external.path(), &link).unwrap();
    sweep(
        data.path(),
        None,
        Some(&HashSet::new()),
        &HashSet::new(),
        &HashSet::new(),
        None,
        now,
    );
    assert!(source.exists());
    assert!(link.exists());
}
