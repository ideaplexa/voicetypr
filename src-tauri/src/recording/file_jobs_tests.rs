use super::*;

fn aged(path: &Path, now: SystemTime, age: Duration) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"audio").unwrap();
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(now - age))
        .unwrap();
}

#[test]
fn sweep_has_24_hour_gate_and_keeps_unrelated_files_and_live_jobs() {
    let data = tempfile::tempdir().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let retry = "cli-recording-20261031123000123_abcdef12.wav";
    let old = data.path().join("cli-recordings").join(retry);
    let legacy = data.path().join("recordings").join(retry);
    let normalized = data
        .path()
        .join("recordings/cli-remote-normalized-20261031123000123_abcdef12.wav");
    let scratch = temp.path().join(".voicetypr_decode_tmp_123456.wav");
    let output = temp
        .path()
        .join(format!("{TEMP_PREFIX}{}.wav", uuid::Uuid::new_v4()));
    let young = data
        .path()
        .join("cli-recordings/cli-recording-20261031123000123_12345678.wav");
    let active = temp.path().join(".voicetypr_decode_tmp_234567.wav");
    let unrelated = [
        temp.path().join(".tmpAnonymousLegacy"),
        temp.path().join("voicetypr-audio-personal.wav"),
        temp.path().join(".voicetypr_decode_tmp_personal.wav"),
        temp.path()
            .join("cli-recording-20261031123000123_abcdef12.wav"),
        data.path().join("cli-recordings/cli-recording-notes.wav"),
        data.path()
            .join("cli-recordings/cli-recording-20261399123000123_abcdef12.wav"),
        data.path().join("recordings/personal.wav"),
    ];
    for path in [&old, &legacy, &normalized, &scratch, &output, &active]
        .into_iter()
        .chain(unrelated.iter())
    {
        aged(path, now, MAX_AGE);
    }
    aged(&young, now, MAX_AGE - Duration::from_secs(1));
    let protected = HashSet::from([active.clone()]);
    assert_eq!(
        sweep(data.path(), temp.path(), &protected, now, false),
        Ok(5)
    );
    for path in [&old, &legacy, &normalized, &scratch, &output] {
        assert!(!path.exists());
    }
    assert!(young.exists());
    assert!(active.exists());
    assert!(unrelated.iter().all(|p| p.exists()));
    assert_eq!(
        sweep(data.path(), temp.path(), &protected, now, true),
        Ok(1)
    );
    assert!(!young.exists());
    assert!(active.exists());
    assert!(unrelated.iter().all(|p| p.exists()));
}

#[cfg(unix)]
#[test]
fn sweep_does_not_follow_symlinks() {
    let data = tempfile::tempdir().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("personal.wav");
    aged(&target, SystemTime::now(), MAX_AGE);
    let link = temp.path().join(".voicetypr_decode_tmp_123.wav");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(
        sweep(
            data.path(),
            temp.path(),
            &HashSet::new(),
            SystemTime::now(),
            true
        ),
        Ok(0)
    );
    assert!(link.exists());
    assert!(target.exists());
}

#[test]
fn retry_refreshes_mtime_for_a_sweeper_without_the_process_local_lease() {
    let data = tempfile::tempdir().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let before_retry = SystemTime::now();
    for directory in ["cli-recordings", "recordings"] {
        let path = data
            .path()
            .join(directory)
            .join("cli-recording-20261031123000123_abcdef12.wav");
        aged(&path, before_retry, MAX_AGE);
        let lease = protect_path(&path);
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert!(modified
            .duration_since(before_retry - Duration::from_secs(1))
            .is_ok());
        // Simulate the desktop sweep while the CLI's lease is invisible to it.
        assert_eq!(
            sweep(
                data.path(),
                temp.path(),
                &HashSet::new(),
                SystemTime::now(),
                false
            ),
            Ok(0)
        );
        assert!(path.exists());
        drop(lease);
        assert_eq!(
            sweep(
                data.path(),
                temp.path(),
                &HashSet::new(),
                modified + MAX_AGE,
                false
            ),
            Ok(1)
        );
    }
}

#[test]
fn refresh_retry_is_best_effort_and_does_not_touch_unrelated_input() {
    let data = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let unrelated = data.path().join("cli-recordings/personal.wav");
    aged(&unrelated, now, MAX_AGE);
    let original = std::fs::metadata(&unrelated).unwrap().modified().unwrap();
    refresh_retry(&unrelated);
    assert_eq!(
        std::fs::metadata(&unrelated).unwrap().modified().unwrap(),
        original
    );
    let missing = data
        .path()
        .join("cli-recordings/cli-recording-20261031123000123_abcdef12.wav");
    let _lease = protect_path(&missing);
    assert!(!missing.exists(), "a failed touch must not create a file");
}

#[test]
fn refresh_retry_accepts_a_bare_relative_filename() {
    let name = format!(
        "cli-recording-20261031123000123_{}",
        &uuid::Uuid::new_v4().to_string()[..8]
    );
    let file = tempfile::Builder::new()
        .prefix(&name)
        .rand_bytes(0)
        .suffix(".wav")
        .tempfile_in(std::env::current_dir().unwrap())
        .unwrap();
    let now = SystemTime::now();
    file.as_file().set_modified(now - MAX_AGE).unwrap();
    refresh_retry(Path::new(file.path().file_name().unwrap()));
    assert!(file
        .as_file()
        .metadata()
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(now - Duration::from_secs(1))
        .is_ok());
}

#[test]
fn reset_reports_failed_deletion_while_periodic_sweep_remains_best_effort() {
    let data = tempfile::tempdir().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let blocked = data
        .path()
        .join("cli-recordings/cli-recording-20261031123000123_abcdef12.wav");
    std::fs::create_dir_all(&blocked).unwrap();
    let removable = temp.path().join(".voicetypr_decode_tmp_123.wav");
    aged(&removable, SystemTime::now(), MAX_AGE);
    assert_eq!(
        sweep(
            data.path(),
            temp.path(),
            &HashSet::new(),
            SystemTime::now(),
            true
        ),
        Err("Could not clear audio storage")
    );
    assert!(
        !removable.exists(),
        "reset continues after a deletion failure"
    );
    assert_eq!(
        sweep(
            data.path(),
            temp.path(),
            &HashSet::new(),
            SystemTime::now(),
            false
        ),
        Ok(0)
    );
    std::fs::remove_dir(blocked).unwrap();
    assert_eq!(
        sweep(
            data.path(),
            temp.path(),
            &HashSet::new(),
            SystemTime::now(),
            true
        ),
        Ok(0),
        "missing storage is not a reset failure"
    );
}

#[test]
fn temp_audio_is_recognizable_owned_and_private() {
    let audio = TempAudio::new().unwrap();
    let path = audio.path().to_owned();
    assert!(temp_name(&path));
    assert!(path.exists());
    super::super::kept::with_protected_paths(|protected| assert!(protected.contains(&path)));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    drop(audio);
    assert!(!path.exists());
}

#[tokio::test]
async fn failed_normalization_removes_its_output() {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let result = normalize_owned(PathBuf::from("unused"), move |_, output| {
        std::fs::write(output, b"partial").unwrap();
        sender.send(output.to_owned()).unwrap();
        Err("decode failed".into())
    })
    .await;
    assert!(result.is_err());
    assert!(!receiver.await.unwrap().exists());
}

#[tokio::test]
async fn cancelled_normalization_owns_output_until_worker_finishes() {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let task = tokio::spawn(normalize_owned(
        PathBuf::from("unused"),
        move |_, output| {
            started_tx.send(output.to_owned()).unwrap();
            release_rx.recv().unwrap();
            std::fs::write(output, b"complete").unwrap();
            Ok(())
        },
    ));
    let path = started_rx.await.unwrap();
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    assert!(path.exists(), "worker must retain its output while writing");
    release_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while path.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
