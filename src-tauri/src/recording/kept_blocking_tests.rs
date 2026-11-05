use super::*;

// Isolated static stores model the production static without mutating STORE.
// Every worker is released and observed before its temporary directory drops.
fn test_store(store: Store) -> &'static Mutex<Store> {
    Box::leak(Box::new(Mutex::new(store)))
}

fn begin(store: &'static Mutex<Store>, id: &str) -> Deletion {
    let DeleteStart::Ready(deletion) = Deletion::begin(store, id).unwrap() else {
        panic!("expected a new deletion");
    };
    *deletion
}

fn retained(data: &Path) -> (&'static Mutex<Store>, String, PathBuf) {
    let source = data.join("source.wav");
    super::super::availability_tests::writer(&source)
        .finalize()
        .unwrap();
    let mut store = Store::default();
    let recovery = store
        .retain(
            &data.join(persistent::DIRECTORY),
            &source,
            7,
            RecoveryKind::Integrity,
        )
        .unwrap();
    let path = store.clips[0].path.clone();
    (test_store(store), recovery.id, path)
}

#[tokio::test]
async fn slow_delete_releases_store_and_the_async_thread_until_commit_finishes() {
    let data = tempfile::tempdir().unwrap();
    let (store, id, path) = retained(data.path());
    let deletion = begin(store, &id);
    let DeleteStart::Waiting(duplicate) = Deletion::begin(store, &id).unwrap() else {
        panic!("duplicate must share the result");
    };
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = tokio::spawn(run_deletion(deletion, move |clip| {
        started.send(()).unwrap();
        blocked.recv_timeout(Duration::from_secs(5)).unwrap();
        let committed = clip.commit_retirement();
        if committed {
            clip.remove_audio();
        }
        committed
    }));
    ready.await.unwrap(); // Cannot run if disk work blocks this async thread.
    {
        let owner = store.try_lock().expect("I/O must not retain STORE");
        assert_eq!(owner.deleting[&id].path, path);
        assert!(owner.deleting[&id].durable);
        assert!(path.exists());
    }
    release.send(()).unwrap();
    task.await.unwrap().unwrap();
    wait_for_deletion(duplicate).await.unwrap();
    assert!(!path.exists());
    let owner = store.lock().unwrap();
    assert!(owner.deleting.is_empty());
    assert!(owner.clips.is_empty());
}

#[tokio::test]
async fn cancelled_delete_awaiter_cannot_abandon_audio_ownership() {
    let data = tempfile::tempdir().unwrap();
    let (store, id, path) = retained(data.path());
    let deletion = begin(store, &id);
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let task = tokio::spawn(run_deletion(deletion, move |_| {
        started.send(()).unwrap();
        blocked.recv_timeout(Duration::from_secs(5)).unwrap();
        false // Failed commit after the invoking renderer/awaiting task is gone.
    }));
    ready.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !store.lock().unwrap().clips.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut owner = store.lock().unwrap();
    assert!(owner.deleting.is_empty());
    assert_eq!(owner.clips[0].recovery.id, id);
    assert!(path.exists());
    owner.clear();
}

#[tokio::test]
async fn failed_async_delete_returns_the_same_error_and_restores_the_original() {
    let data = tempfile::tempdir().unwrap();
    let files = super::super::super::storage::CaptureFiles::create(data.path()).unwrap();
    let source = files.audio_path();
    let writer = super::super::availability_tests::writer(&source);
    let mut original = Store::default();
    let recovery = original
        .retain(
            &data.path().join(persistent::DIRECTORY),
            &source,
            7,
            RecoveryKind::Integrity,
        )
        .unwrap();
    let index = original.clips[0].target.with_extension("json");
    std::fs::remove_file(&index).unwrap();
    std::fs::create_dir(&index).unwrap(); // Intent replacement and fallback both fail.
    let store = test_store(original);
    let deletion = begin(store, &recovery.id);
    let DeleteStart::Waiting(duplicate) = Deletion::begin(store, &recovery.id).unwrap() else {
        panic!("duplicate must share the result");
    };
    {
        let mut owner = store.lock().unwrap();
        assert_eq!(
            owner.clear_for_reset(),
            Err("Could not clear audio storage".into())
        );
        assert!(matches!(
            owner.poll_finalization(&recovery.id, Instant::now()),
            Some((false, None))
        ));
    }
    assert_eq!(
        run_deletion(deletion, Clip::commit_retirement).await,
        Err(DELETE_ERROR.into())
    );
    assert_eq!(wait_for_deletion(duplicate).await, Err(DELETE_ERROR.into()));
    let mut owner = store.lock().unwrap();
    assert_eq!(owner.clips[0].recovery.id, recovery.id);
    assert!(!owner.clips[0].busy);
    assert!(owner.deleting.is_empty());
    assert!(source.exists());
    std::fs::remove_dir(index).unwrap();
    writer.finalize().unwrap();
    let probes = probe_availability(owner.availability_paths(None));
    owner.apply_availability(probes);
    assert!(matches!(
        owner.poll_finalization(&recovery.id, Instant::now()),
        Some((true, _))
    ));
    assert_eq!(owner.clips[0].path, owner.clips[0].target);
    owner.clear();
}

#[tokio::test]
async fn panicked_delete_worker_restores_the_owner() {
    let data = tempfile::tempdir().unwrap();
    let (store, id, path) = retained(data.path());
    let deletion = begin(store, &id);
    assert_eq!(
        run_deletion(deletion, |_| panic!("injected commit failure")).await,
        Err(DELETE_ERROR.into())
    );
    let mut owner = store.lock().unwrap();
    assert_eq!(owner.clips[0].recovery.id, id);
    assert!(owner.deleting.is_empty());
    assert!(path.exists());
    owner.clear();
}

#[test]
fn blocked_exit_io_releases_store_and_cannot_extend_the_exit_wait() {
    let data = tempfile::tempdir().unwrap();
    let (store, _, path) = retained(data.path());
    let worker_store = store;
    let (release, blocked) = std::sync::mpsc::channel();
    let (done, finished) = std::sync::mpsc::channel();
    let start = Instant::now();
    assert!(!run_exit_work(Duration::from_millis(20), move |deadline| {
        let mut owned = take_for_exit(worker_store);
        assert!(!claim_terminal(&owned.clips[0].terminal));
        blocked.recv_timeout(Duration::from_secs(5)).unwrap();
        owned.preserve_on_exit_until(deadline);
        owned.clear();
        done.send(()).unwrap();
    }));
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(store.try_lock().is_ok());
    release.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(store.lock().unwrap().closed);
    assert!(path.exists(), "deadline expiry is not consent to delete");
}

#[test]
fn quit_cannot_unlink_a_failed_pending_delete_through_its_handoff_fallback() {
    let data = tempfile::tempdir().unwrap();
    let files = super::super::super::storage::CaptureFiles::create(data.path()).unwrap();
    let source = files.audio_path();
    let writer = super::super::availability_tests::writer(&source);
    let mut original = Store::default();
    let recovery = original
        .retain(
            &data.path().join(persistent::DIRECTORY),
            &source,
            7,
            RecoveryKind::Integrity,
        )
        .unwrap();
    let index = original.clips[0].target.with_extension("json");
    original
        .fallback
        .push((source.clone(), Some(files.clone())));
    let store = test_store(original);
    let deletion = begin(store, &recovery.id);
    writer.finalize().unwrap(); // clear() would now be allowed to unlink this.
    let mut owned = take_for_exit(store);
    owned.preserve_on_exit_until(Instant::now());
    owned.clear();
    drop(deletion); // Intent never committed, and the closed store cannot restore.
    drop(files);
    assert!(source.exists());
    assert!(index.exists());
    assert!(store.lock().unwrap().clips.is_empty());
}

#[test]
fn blocked_store_lock_cannot_extend_the_exit_wait() {
    let store = test_store(Store::default());
    let held = store.lock().unwrap();
    let worker_store = store;
    let (done, finished) = std::sync::mpsc::channel();
    let start = Instant::now();
    assert!(!run_exit_work(Duration::from_millis(20), move |_| {
        drop(take_for_exit(worker_store));
        done.send(()).unwrap();
    }));
    assert!(start.elapsed() < Duration::from_secs(1));
    drop(held);
    finished.recv_timeout(Duration::from_secs(5)).unwrap();
}
