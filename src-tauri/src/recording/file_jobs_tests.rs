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
    assert_eq!(sweep(data.path(), temp.path(), &protected, now, false), 5);
    for path in [&old, &legacy, &normalized, &scratch, &output] {
        assert!(!path.exists());
    }
    assert!(young.exists());
    assert!(active.exists());
    assert!(unrelated.iter().all(|p| p.exists()));
    assert_eq!(sweep(data.path(), temp.path(), &protected, now, true), 1);
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
        0
    );
    assert!(link.exists());
    assert!(target.exists());
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
