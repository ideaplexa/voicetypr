use super::*;
use crate::recording::kept::with_protected_paths;
use std::time::Duration;

#[tokio::test]
async fn successful_normalization_returns_owned_uuid_named_audio() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.wav");
    let mut writer = hound::WavWriter::create(
        &input,
        hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    writer.write_sample(1_i16).unwrap();
    writer.finalize().unwrap();
    let output = normalize_upload_audio(dir.path(), &input).await.unwrap();
    let path = output.path().to_owned();
    let id = path
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("upload-normalized-")
        .unwrap();
    assert!(uuid::Uuid::parse_str(id).is_ok());
    assert_eq!(
        std::fs::read(&input).unwrap(),
        std::fs::read(&path).unwrap()
    );
    with_protected_paths(|paths| assert!(paths.contains(&path)));
    drop(output);
    assert!(input.exists());
    assert!(!path.exists());
    with_protected_paths(|paths| assert!(!paths.contains(&path)));
}

#[tokio::test]
async fn failed_normalization_removes_partial_output_and_releases_both_leases() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.wav");
    let output = dir.path().join("output.wav");
    std::fs::write(&input, b"original").unwrap();
    let result = normalize_owned(input.clone(), output.clone(), |_, path| {
        std::fs::write(path, b"partial").unwrap();
        Err("decode failed".into())
    })
    .await;
    assert!(result.is_err());
    assert!(input.exists());
    assert!(!output.exists());
    with_protected_paths(|paths| {
        assert!(!paths.contains(&input));
        assert!(!paths.contains(&output));
    });
}

#[tokio::test]
async fn dropping_normalization_future_keeps_worker_owned_until_its_result_is_discarded() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.wav");
    let output = dir.path().join("output.wav");
    std::fs::write(&input, b"original").unwrap();
    let (ready, started) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = tokio::spawn(normalize_owned(
        input.clone(),
        output.clone(),
        move |_, path| {
            std::fs::write(path, b"partial").unwrap();
            ready.send(()).unwrap();
            blocked.recv_timeout(Duration::from_secs(15)).unwrap();
            // Simulate the decoder finishing the output after its caller is gone.
            std::fs::write(path, b"complete").unwrap();
            Ok(())
        },
    ));
    started.await.unwrap();
    task.abort();
    assert!(task.await.err().unwrap().is_cancelled());
    assert!(
        output.exists(),
        "cancellation must not unlink a worker's live output"
    );
    with_protected_paths(|paths| {
        assert!(paths.contains(&input));
        assert!(paths.contains(&output));
    });
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let released =
                with_protected_paths(|paths| !paths.contains(&input) && !paths.contains(&output));
            if released && !output.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("discarded blocking-job result must remove its owned output");
    assert!(input.exists());
}
