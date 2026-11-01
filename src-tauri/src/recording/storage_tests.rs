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
    let lease = super::super::kept::protect_path(&upload);
    let run = || {
        super::super::kept::with_protected_paths(|protected| {
            sweep(
                data.path(),
                None,
                Some(&HashSet::new()),
                protected,
                &HashSet::new(),
                Some(7),
                now,
            )
        })
    };
    let counts = run();
    assert_eq!(counts.orphans, 0);
    assert_eq!(counts.expired, 0);
    assert!(
        upload.exists(),
        "file-job ownership can outlive the capture orphan grace"
    );
    drop(lease);
    assert_eq!(run().orphans, 1);
    assert!(!upload.exists());
}

#[test]
fn upload_orphans_require_at_least_twenty_four_hours_and_a_valid_name() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let upload = || {
        data.path()
            .join("recordings")
            .join(format!("upload-normalized-{}.wav", uuid::Uuid::new_v4()))
    };
    let recent = upload();
    let boundary = upload();
    let abandoned = upload();
    let unrelated = data
        .path()
        .join("recordings/upload-normalized-personal.wav");
    write_aged(&recent, now, FILE_JOB_ORPHAN_AGE - Duration::from_secs(1));
    write_aged(&boundary, now, FILE_JOB_ORPHAN_AGE);
    write_aged(
        &abandoned,
        now,
        FILE_JOB_ORPHAN_AGE + Duration::from_secs(1),
    );
    write_aged(&unrelated, now, Duration::from_secs(86400 * 40));
    let counts = sweep(
        data.path(),
        None,
        None,
        &HashSet::new(),
        &HashSet::new(),
        None,
        now,
    );
    assert_eq!(counts.orphans, 2);
    assert_eq!(counts.expired, 0);
    assert!(recent.exists());
    assert!(unrelated.exists());
    assert!(!boundary.exists());
    assert!(!abandoned.exists());
}

#[test]
fn real_decoder_scratch_names_use_capture_or_file_job_age_as_appropriate() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let capture = capture_path(data.path(), "normalized_20260101_120000.wav");
    let recording = data.path().join("recordings/output.wav");
    // Mirror the decoder: create each file before requesting another timestamp.
    // Back-to-back names can coincide on microsecond-resolution host clocks.
    let capture_old = crate::audio::decode::temp_path_for(&capture);
    write_aged(&capture_old, now, ORPHAN_AGE);
    let capture_recent = crate::audio::decode::temp_path_for(&capture);
    write_aged(&capture_recent, now, ORPHAN_AGE - Duration::from_secs(1));
    let recording_old = crate::audio::decode::temp_path_for(&recording);
    write_aged(&recording_old, now, FILE_JOB_ORPHAN_AGE);
    let recording_recent = crate::audio::decode::temp_path_for(&recording);
    write_aged(
        &recording_recent,
        now,
        FILE_JOB_ORPHAN_AGE - Duration::from_secs(1),
    );
    let recording_active = crate::audio::decode::temp_path_for(&recording);
    write_aged(&recording_active, now, Duration::from_secs(86400 * 40));
    assert_ne!(capture_old, capture_recent);
    assert_ne!(recording_old, recording_recent);
    assert_ne!(recording_old, recording_active);
    assert_ne!(recording_recent, recording_active);
    let unrelated = capture_old.with_file_name(format!(
        "{}x.wav",
        capture_old.file_stem().unwrap().to_str().unwrap()
    ));
    write_aged(&unrelated, now, Duration::from_secs(86400 * 40));
    let _lease = super::super::kept::protect_path(&recording_active);
    let counts = super::super::kept::with_protected_paths(|protected| {
        sweep(
            data.path(),
            None,
            Some(&HashSet::new()),
            protected,
            &HashSet::new(),
            None,
            now,
        )
    });
    assert_eq!(counts.orphans, 2);
    assert!(!capture_old.exists());
    assert!(!recording_old.exists());
    for kept in [
        &capture_recent,
        &recording_recent,
        &recording_active,
        &unrelated,
    ] {
        assert!(kept.exists());
    }
}

#[test]
fn overlapping_history_retries_protect_original_wav_until_last_completion() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let source = data
        .path()
        .join("recordings/2026-01-01_12-00-00_12345678.wav");
    write_aged(&source, now, Duration::from_secs(86400 * 8));
    let refs = HashSet::from([source.file_name().unwrap().to_str().unwrap().to_owned()]);
    let run = || {
        super::super::kept::with_protected_paths(|protected| {
            sweep(
                data.path(),
                None,
                Some(&refs),
                protected,
                &HashSet::new(),
                Some(7),
                now,
            )
        })
    };
    let first = super::super::kept::protect_path(&source);
    let second = super::super::kept::protect_path(&source);
    assert_eq!(run().expired, 0);
    drop(first);
    assert_eq!(run().expired, 0);
    assert!(source.exists());
    drop(second);
    assert_eq!(run().expired, 1);
    assert!(!source.exists());
}

#[test]
fn history_retry_error_releases_source_and_registration_precedes_first_await() {
    let data = tempfile::tempdir().unwrap();
    let source = data
        .path()
        .join("recordings/2026-01-01_12-00-00_12345678.wav");
    let now = SystemTime::now();
    write_aged(&source, now, Duration::from_secs(86400 * 8));
    let result = (|| -> Result<(), ()> {
        let _lease = super::super::kept::protect_path(&source);
        Err(())?;
        Ok(())
    })();
    assert!(result.is_err());
    let counts = super::super::kept::with_protected_paths(|protected| {
        assert!(!protected.contains(&source));
        sweep(
            data.path(),
            None,
            Some(&HashSet::new()),
            protected,
            &HashSet::new(),
            Some(7),
            now,
        )
    });
    assert_eq!(counts.expired, 1);
    assert!(!source.exists());
    let entry = include_str!("../commands/audio.rs")
        .split_once("pub(crate) async fn transcribe_audio_file_impl(")
        .unwrap()
        .1;
    assert!(entry.split_once(".await").unwrap().0.contains(
        "let _source_lease = crate::recording::kept::protect_path(Path::new(&file_path));"
    ));
}

#[tokio::test]
async fn cancelled_history_retry_releases_its_original_saved_source() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let source = data
        .path()
        .join("recordings/2026-01-01_12-00-00_12345678.wav");
    write_aged(&source, now, Duration::from_secs(86400 * 8));
    let path = source.clone();
    let (ready, started) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _source_lease = super::super::kept::protect_path(&path);
        ready.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    started.await.unwrap();
    let run = || {
        super::super::kept::with_protected_paths(|protected| {
            sweep(
                data.path(),
                None,
                Some(&HashSet::new()),
                protected,
                &HashSet::new(),
                Some(7),
                now,
            )
        })
    };
    assert_eq!(run().expired, 0);
    assert!(source.exists());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(run().expired, 1);
    assert!(!source.exists());
}

#[test]
fn remote_history_retry_registers_source_before_waiting_for_settings() {
    let entry = include_str!("../commands/remote.rs")
        .split_once("pub async fn transcribe_remote(")
        .unwrap()
        .1;
    let before_await = entry.split_once(".await").unwrap().0;
    assert!(
        before_await.contains(
            "let _source_lease = crate::recording::kept::protect_path(std::path::Path::new(&audio_path));"
        ),
        "remote History retries must protect the original (non-canonical) maintenance path before awaiting settings"
    );
}

#[tokio::test]
async fn cancelled_take_defers_cleanup_until_detached_decoder_releases_its_scratch() {
    let data = tempfile::tempdir().unwrap();
    let files = CaptureFiles::create(data.path()).unwrap();
    files.writer_stopped(false);
    let scratch = crate::audio::decode::temp_path_for(&files.audio_path());
    std::fs::write(&scratch, b"partial").unwrap();
    let worker_lease = super::super::kept::protect_path(&scratch);
    drop(files);
    assert!(scratch.exists());
    super::super::kept::with_protected_paths(|protected| {
        let active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        sweep(
            data.path(),
            None,
            Some(&HashSet::new()),
            protected,
            &active.keys().cloned().collect(),
            None,
            SystemTime::now() + Duration::from_secs(86400),
        );
    });
    assert!(scratch.exists());
    drop(worker_lease);
    wait_removed(scratch.parent().unwrap()).await;
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
    let upload = data
        .path()
        .join("recordings")
        .join(format!("upload-normalized-{}.wav", uuid::Uuid::new_v4()));
    let scratch = crate::audio::decode::temp_path_for(&saved);
    let capture_scratch = crate::audio::decode::temp_path_for(&abandoned);
    for p in [
        &active,
        &abandoned,
        &saved,
        &recovery,
        &old_recovery,
        &upload,
        &scratch,
        &capture_scratch,
    ] {
        write_aged(p, SystemTime::now(), Duration::ZERO);
    }
    clear_storage(data.path(), Some(legacy.path().to_owned())).unwrap();
    assert!(active.exists());
    for p in [
        &abandoned,
        &saved,
        &recovery,
        &old_recovery,
        &upload,
        &scratch,
        &capture_scratch,
    ] {
        assert!(!p.exists());
    }
    files.writer_stopped(false);
    drop(files);
    wait_removed(active.parent().unwrap()).await;
}

#[test]
fn cli_capture_has_no_short_orphan_age_gate_in_either_desktop_sweep() {
    let data = tempfile::tempdir().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let path = data
        .path()
        .join("cli-recordings/cli-recording-20261031123000123_abcdef12.wav");
    write_aged(&path, now, ORPHAN_AGE + Duration::from_secs(1));
    sweep(
        data.path(),
        None,
        Some(&HashSet::new()),
        &HashSet::new(),
        &HashSet::new(),
        Some(0),
        now,
    );
    assert_eq!(
        super::super::file_jobs::sweep(data.path(), temp.path(), &HashSet::new(), now, false),
        Ok(0)
    );
    assert!(
        path.exists(),
        "CLI captures use the 24-hour last-write gate, not ten minutes"
    );
}

#[test]
fn reset_reports_cli_and_capture_deletion_failures_and_keeps_clearing_other_storage() {
    for blocked_name in [
        "cli-recordings/cli-recording-20261031123000123_abcdef12.wav",
        "captures/abandoned/recording.wav",
    ] {
        let data = tempfile::tempdir().unwrap();
        let blocked = data.path().join(blocked_name);
        std::fs::create_dir_all(&blocked).unwrap();
        let saved = data.path().join("recordings/saved.wav");
        write_aged(&saved, SystemTime::now(), Duration::ZERO);
        assert_eq!(
            clear_storage(data.path(), None),
            Err("Could not clear audio storage".into())
        );
        assert!(blocked.exists());
        assert!(
            !saved.exists(),
            "one failure must not prevent other cleanup"
        );
        std::fs::remove_dir(blocked).unwrap();
        clear_storage(data.path(), None).unwrap();
    }
}

#[test]
fn reset_reports_unreadable_storage_roots() {
    for name in ["cli-recordings", "captures", "recordings", "recovery"] {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(name), b"not a directory").unwrap();
        assert_eq!(
            clear_storage(data.path(), None),
            Err("Could not clear audio storage".into())
        );
    }
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
