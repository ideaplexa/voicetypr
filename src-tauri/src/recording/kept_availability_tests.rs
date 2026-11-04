use super::*;
fn snapshots(store: &mut Store, now: Instant) -> Vec<serde_json::Value> {
    let probes = probe_availability(store.availability_paths());
    store.apply_availability(probes);
    store.expire(now);
    store.live_recoveries(now)
}

pub(super) fn writer(path: &Path) -> hound::WavWriter<std::io::BufWriter<std::fs::File>> {
    let mut writer = hound::WavWriter::create(
        path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    writer.write_sample(42_i16).unwrap();
    writer
}

#[test]
fn missing_audio_cannot_be_registered_as_a_pending_retry() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::default();
    let result = store.retain(
        dir.path(),
        &dir.path().join("missing.wav"),
        7,
        RecoveryKind::Integrity,
    );
    assert_eq!(
        result.err().as_deref(),
        Some("Recording expired or discarded")
    );
    assert!(store.clips.is_empty());
}

#[test]
fn live_writer_is_kept_but_only_finalized_audio_offers_retry() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("capture.wav");
    let writer = writer(&source);
    let mut store = Store::default();
    let recovery = store
        .retain(dir.path(), &source, 7, RecoveryKind::Integrity)
        .unwrap();
    assert!(!recovery.retry_ready);
    assert_eq!(
        snapshots(&mut store, Instant::now())[0]["retry_ready"],
        false
    );
    assert!(source.exists(), "pending audio must not be lost");
    writer.finalize().unwrap();
    assert!(store.promote(&recovery.id));
    assert!(snapshots(&mut store, Instant::now())[0]["retry_ready"] == true);
    assert!(store.clips[0].path.exists());
    store.clear();
}

#[test]
fn hydration_uses_remaining_lease_and_cannot_revive_removed_audio() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("capture.wav");
    writer(&source).finalize().unwrap();
    let mut store = Store::default();
    let recovery = store
        .retain(dir.path(), &source, 7, RecoveryKind::Integrity)
        .unwrap();
    let start = Instant::now();
    store.clips[0].expires = start + Duration::from_secs(10);
    crate::commands::pill_feedback::with_queue_for_test(|queue| {
        queue.push((
            "dictation-recovery",
            serde_json::to_value(&recovery).unwrap(),
        ))
    });
    let live = snapshots(&mut store, start + Duration::from_secs(2));
    assert_eq!(live[0]["expires_in_ms"], 8_000);
    assert!(live[0]["retry_ready"] == true);
    std::fs::remove_file(&store.clips[0].path).unwrap();
    assert!(snapshots(&mut store, start + Duration::from_secs(2)).is_empty());
    assert!(!crate::commands::pill_feedback::with_queue_for_test(
        |queue| queue
            .iter()
            .any(|(event, payload)| *event == "dictation-recovery"
                && payload["id"].as_str() == Some(recovery.id.as_str()))
    ));
}

#[test]
fn delayed_hydration_does_not_extend_expiry_even_for_older_generations() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("capture.wav");
    writer(&source).finalize().unwrap();
    let mut store = Store::default();
    let recovery = store
        .retain(dir.path(), &source, 1, RecoveryKind::NoSpeech)
        .unwrap();
    let path = store.clips[0].path.clone();
    let deadline = store.clips[0].expires;
    let live = snapshots(&mut store, deadline - Duration::from_secs(1));
    assert_eq!(live[0]["id"], recovery.id);
    assert_eq!(
        live[0]["generation"], 1,
        "age of the take is not lease validity"
    );
    assert_eq!(live[0]["expires_in_ms"], 1_000);
    assert!(snapshots(&mut store, deadline).is_empty());
    assert!(!path.exists());
}

#[tokio::test]
async fn removed_engine_has_a_safe_reason_and_private_provider_errors_are_not_forwarded() {
    for (error, expected) in [
        (
            "Requested local engine is not ready",
            RetryError::EngineUnavailable,
        ),
        (
            "/private/recording.wav?provider-token=secret",
            RetryError::Failed,
        ),
    ] {
        let (_cancel, cancelled) = tokio::sync::watch::channel(false);
        let result = cancellable_retry(cancelled, async { Err::<(), String>(error.into()) }).await;
        assert_eq!(result, Err(expected));
    }
}

#[test]
fn old_unlocked_probe_cannot_overwrite_a_finalizer_readiness_publication() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("capture.wav");
    let writer = writer(&source);
    let mut store = Store::default();
    store
        .retain(dir.path(), &source, 7, RecoveryKind::Integrity)
        .unwrap();
    let probes = probe_availability(store.availability_paths());
    writer.finalize().unwrap();
    // A finalizer can publish readiness without moving the source (e.g. rename fails).
    store.clips[0].recovery.retry_ready = true;
    store.apply_availability(probes);
    assert!(store.clips[0].recovery.retry_ready);
    store.clear();
}

#[test]
fn a_probe_winning_the_cache_race_does_not_suppress_finalization_notification() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("capture.wav");
    let writer = writer(&source);
    let mut store = Store::default();
    let recovery = store
        .retain(dir.path(), &source, 7, RecoveryKind::Integrity)
        .unwrap();
    store.clips[0].announced = true; // The renderer currently has a finishing card.
    writer.finalize().unwrap();
    let probes = probe_availability(store.availability_paths());
    store.apply_availability(probes);
    assert!(store.clips[0].recovery.retry_ready);
    let (ready, publication) = store
        .poll_finalization(&recovery.id, Instant::now())
        .unwrap();
    assert!(ready);
    let publication = publication.expect("cached readiness is not an emitted notification");
    assert!(publication.retry_ready);
    assert_eq!(publication.id, recovery.id);
    store.clear();
}
