//! Transactional, platform-neutral ONNX model download.
#![cfg_attr(
    not(all(target_os = "windows", target_arch = "x86_64")),
    allow(dead_code)
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client;
use sha2::{Digest, Sha256};
use sysinfo::Disks;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub const MODEL_ID: &str = "parakeet-tdt-0.6b-v3";
pub const REVISION: &str = "8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce";
const REPO: &str = "istupakov/parakeet-tdt-0.6b-v3-onnx";
const MARKER: &str = ".verified-revision";
const STALL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug)]
pub struct ModelFile {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

pub struct Manifest<'a> {
    pub revision: &'a str,
    pub files: &'a [ModelFile],
}

pub const FILES: [ModelFile; 3] = [
    ModelFile {
        name: "encoder-model.int8.onnx",
        size: 652_183_999,
        sha256: "6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
    },
    ModelFile {
        name: "decoder_joint-model.int8.onnx",
        size: 18_202_004,
        sha256: "eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
    },
    ModelFile {
        name: "vocab.txt",
        size: 93_939,
        sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
    },
];
pub const PINNED: Manifest<'static> = Manifest {
    revision: REVISION,
    files: &FILES,
};

pub fn total_size(files: &[ModelFile]) -> u64 {
    files.iter().map(|file| file.size).sum()
}

pub fn model_directory(root: &Path) -> PathBuf {
    root.join(MODEL_ID)
}

pub fn is_downloaded(directory: &Path, revision: &str, files: &[ModelFile]) -> bool {
    if !directory.is_dir()
        || std::fs::read_to_string(directory.join(MARKER))
            .ok()
            .as_deref()
            != Some(revision)
    {
        return false;
    }
    files.iter().all(|file| {
        directory
            .join(file.name)
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() == file.size)
    })
}

fn available_space(parent: &Path) -> Result<u64, String> {
    let canonical = parent
        .canonicalize()
        .map_err(|_| "Cannot check free disk space for the Parakeet model directory".to_string())?;
    // Windows canonical paths may carry a verbatim `\\?\` prefix while
    // sysinfo reports drive mount points as ordinary `C:\` paths.
    #[cfg(windows)]
    let canonical_match = canonical
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .to_ascii_lowercase();
    Disks::new_with_refreshed_list()
        .list()
        .iter()
        .filter(|disk| {
            #[cfg(windows)]
            {
                let mount = disk.mount_point().to_string_lossy().to_ascii_lowercase();
                canonical_match.starts_with(mount.as_str())
            }
            #[cfg(not(windows))]
            {
                canonical.starts_with(disk.mount_point())
            }
        })
        .max_by_key(|disk| disk.mount_point().components().count())
        .map(|disk| disk.available_space())
        .ok_or_else(|| "Cannot check free disk space for the Parakeet model directory".into())
}

async fn remove_directory(path: &Path, purpose: &str) -> Result<(), String> {
    for attempt in 0..3 {
        match fs::remove_dir_all(path).await {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) if attempt < 2 => {
                tokio::time::sleep(Duration::from_millis(50 * (attempt + 1))).await;
            }
            Err(_) => {
                return Err(format!(
                    "Cannot remove the Parakeet {purpose}. Close programs using the model files and try again."
                ));
            }
        }
    }
    unreachable!()
}

async fn reclaim_transactions(root: &Path, manifest: &Manifest<'_>) -> Result<(), String> {
    let destination = model_directory(root);
    let mut entries = fs::read_dir(root)
        .await
        .map_err(|_| "Cannot inspect the Parakeet model directory".to_string())?;
    let staging_prefix = format!(".staging-{MODEL_ID}-");
    let replaced_prefix = format!(".replaced-{MODEL_ID}-");
    let mut staging = Vec::new();
    let mut backups = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| "Cannot inspect the Parakeet model directory".to_string())?
    {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&staging_prefix) {
            staging.push(entry.path());
        } else if name.starts_with(&replaced_prefix) {
            backups.push(entry.path());
        }
    }
    for path in staging {
        remove_directory(&path, "leftover download transaction").await?;
    }
    // A marker is the publication point. Never replace a committed destination.
    if !destination.join(MARKER).exists() {
        backups.sort();
        if let Some(backup) = backups
            .iter()
            .rev()
            .find(|path| is_downloaded(path, manifest.revision, manifest.files))
        {
            remove_directory(&destination, "uncommitted model").await?;
            fs::rename(backup, &destination).await.map_err(|_| {
                "Cannot restore the previous Parakeet model. Try downloading again.".to_string()
            })?;
        } else {
            remove_directory(&destination, "uncommitted model").await?;
        }
    }
    for path in backups {
        remove_directory(&path, "leftover download transaction").await?;
    }
    Ok(())
}

pub async fn recover_on_startup(
    root: &Path,
    operation_lock: &tokio::sync::Mutex<()>,
) -> Result<(), String> {
    let _guard = operation_lock.lock().await;
    fs::create_dir_all(root)
        .await
        .map_err(|_| "Cannot inspect the Parakeet model directory".to_string())?;
    reclaim_transactions(root, &PINNED).await
}

#[derive(Clone, Copy)]
enum PublishPoint {
    BeforeCommit,
    AfterCommit,
    BeforeRollback,
}

fn cancelled(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::SeqCst) {
        Err("Download cancelled by user".into())
    } else {
        Ok(())
    }
}

async fn lock_gate_with_cancel<'a>(
    gate: &'a tokio::sync::Mutex<()>,
    cancel: &AtomicBool,
) -> Result<tokio::sync::MutexGuard<'a, ()>, String> {
    loop {
        cancelled(cancel)?;
        tokio::select! {
            guard = gate.lock() => {
                cancelled(cancel)?;
                return Ok(guard);
            }
            () = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
}

async fn rollback(destination: &Path, backup: &Path, old_exists: bool) -> Result<(), String> {
    // Invalidate first, even if Windows refuses to remove the new directory.
    match fs::remove_file(destination.join(MARKER)).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Rollback failed to invalidate the Parakeet model marker".into()),
    }
    remove_directory(destination, "uncommitted model during rollback")
        .await
        .map_err(|error| format!("Rollback failed: {error}"))?;
    if old_exists {
        fs::rename(backup, destination)
            .await
            .map_err(|_| "Rollback failed to restore the previous Parakeet model".to_string())?;
    }
    Ok(())
}

async fn with_cancel<T>(
    flag: &AtomicBool,
    operation: impl std::future::Future<Output = Result<T, reqwest::Error>>,
    stall: Duration,
) -> Result<T, String> {
    let mut operation = Box::pin(operation);
    let deadline = tokio::time::sleep(stall);
    tokio::pin!(deadline);
    loop {
        cancelled(flag)?;
        tokio::select! {
            result = &mut operation => return result.map_err(|_| "Parakeet download failed. Check your connection and try again.".into()),
            () = &mut deadline => return Err("Parakeet download stalled. Check your connection and try again.".into()),
            () = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }
}

/// Downloads to a sibling staging directory and publishes only a fully verified set.
/// `url_for` is injected so tests can use a local HTTP server.
pub struct DownloadRequest<'a> {
    pub client: &'a Client,
    pub root: &'a Path,
    pub manifest: &'a Manifest<'a>,
    pub operation_lock: &'a tokio::sync::Mutex<()>,
    pub gate: &'a tokio::sync::Mutex<()>,
    pub cancel: Arc<AtomicBool>,
    pub stall: Duration,
}

pub async fn download(
    request: DownloadRequest<'_>,
    url_for: impl Fn(&str) -> String,
    unload: impl std::future::Future<Output = ()>,
    progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    download_with_hook(request, url_for, unload, progress, |_| {}).await
}

async fn download_with_hook(
    request: DownloadRequest<'_>,
    url_for: impl Fn(&str) -> String,
    unload: impl std::future::Future<Output = ()>,
    mut progress: impl FnMut(u64, u64),
    hook: impl Fn(PublishPoint),
) -> Result<(), String> {
    let DownloadRequest {
        client,
        root,
        manifest,
        operation_lock,
        gate,
        cancel,
        stall,
    } = request;
    let _operation = operation_lock.lock().await;
    let destination = model_directory(root);
    let total = total_size(manifest.files);
    fs::create_dir_all(root)
        .await
        .map_err(|_| "Cannot create the Parakeet model directory".to_string())?;
    reclaim_transactions(root, manifest).await?;
    cancelled(&cancel)?;
    if is_downloaded(&destination, manifest.revision, manifest.files) {
        progress(total, total);
        return Ok(());
    }
    let required = total.saturating_mul(2);
    if available_space(root)? < required {
        return Err(format!(
            "Parakeet needs about {} MB of free disk space to download and install. Free space and try again.",
            required.div_ceil(1_000_000)
        ));
    }
    let nonce = uuid::Uuid::new_v4();
    let staging = root.join(format!(".staging-{MODEL_ID}-{nonce}"));
    fs::create_dir(&staging)
        .await
        .map_err(|_| "Cannot create the Parakeet download staging directory".to_string())?;
    let mut committed = false;
    let result = async {
        let mut downloaded = 0u64;
        for file in manifest.files {
            cancelled(&cancel)?;
            let response =
                with_cancel(&cancel, client.get(url_for(file.name)).send(), stall).await?;
            if !response.status().is_success() {
                return Err(format!(
                    "Parakeet download returned HTTP {}. Try again later.",
                    response.status().as_u16()
                ));
            }
            if response
                .content_length()
                .is_some_and(|length| length != file.size)
            {
                return Err(format!(
                    "Parakeet file {} has the wrong size. Try again.",
                    file.name
                ));
            }
            let mut response = response;
            let mut output = fs::File::create(staging.join(file.name))
                .await
                .map_err(|_| {
                    "Cannot write the Parakeet download. Check free disk space.".to_string()
                })?;
            let mut size = 0u64;
            let mut hash = Sha256::new();
            while let Some(chunk) = with_cancel(&cancel, response.chunk(), stall).await? {
                cancelled(&cancel)?;
                size = size.saturating_add(chunk.len() as u64);
                if size > file.size {
                    return Err(format!(
                        "Parakeet file {} has the wrong size. Try again.",
                        file.name
                    ));
                }
                output.write_all(&chunk).await.map_err(|_| {
                    "Cannot write the Parakeet download. Check free disk space.".to_string()
                })?;
                hash.update(&chunk);
                downloaded += chunk.len() as u64;
                progress(downloaded, total);
            }
            cancelled(&cancel)?;
            // tokio's sync_all can succeed while an earlier buffered write failed;
            // flush first so a disk-full error surfaces here.
            output.flush().await.map_err(|_| {
                "Cannot save the Parakeet download. Check free disk space.".to_string()
            })?;
            output.sync_all().await.map_err(|_| {
                "Cannot save the Parakeet download. Check free disk space.".to_string()
            })?;
            if size != file.size {
                return Err(format!(
                    "Parakeet file {} has the wrong size. Try again.",
                    file.name
                ));
            }
            if format!("{:x}", hash.finalize()) != file.sha256 {
                return Err(format!(
                    "Parakeet file {} failed verification. Try again.",
                    file.name
                ));
            }
        }
        cancelled(&cancel)?;
        let _gate = lock_gate_with_cancel(gate, &cancel).await?;
        unload.await;
        // Another operation or process may have installed a complete revision
        // while bytes were in flight. Recheck while publication is protected.
        if is_downloaded(&destination, manifest.revision, manifest.files) {
            progress(total, total);
            return Ok(());
        }
        cancelled(&cancel)?;
        let backup = root.join(format!(".replaced-{MODEL_ID}-{nonce}"));
        let old_exists = destination.exists();
        if old_exists {
            fs::rename(&destination, &backup).await.map_err(|_| {
                "Cannot replace the old Parakeet model. Close the app and try again.".to_string()
            })?;
        }
        let publish_result = async {
            fs::rename(&staging, &destination)
                .await
                .map_err(|_| "Cannot install the Parakeet model. Try again.".to_string())?;
            let temporary_marker = destination.join(format!("{MARKER}.tmp-{nonce}"));
            let mut marker = fs::File::create(&temporary_marker)
                .await
                .map_err(|_| "Cannot write the Parakeet model marker".to_string())?;
            marker
                .write_all(manifest.revision.as_bytes())
                .await
                .map_err(|_| "Cannot write the Parakeet model marker".to_string())?;
            marker
                .flush()
                .await
                .map_err(|_| "Cannot write the Parakeet model marker".to_string())?;
            marker
                .sync_all()
                .await
                .map_err(|_| "Cannot sync the Parakeet model marker".to_string())?;
            drop(marker);
            hook(PublishPoint::BeforeCommit);
            cancelled(&cancel)?;
            fs::rename(&temporary_marker, destination.join(MARKER))
                .await
                .map_err(|_| "Cannot commit the Parakeet model marker".to_string())?;
            committed = true;
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = publish_result {
            hook(PublishPoint::BeforeRollback);
            if let Err(rollback_error) = rollback(&destination, &backup, old_exists).await {
                return Err(format!("{error}; {rollback_error}"));
            }
            return Err(error);
        }
        drop(_gate);
        hook(PublishPoint::AfterCommit);
        // The marker rename committed the install. Cleanup cannot turn success
        // into a reported failure; restart recovery will retry any leftover backup.
        if old_exists {
            if let Err(error) = remove_directory(&backup, "previous model backup").await {
                log::warn!("{error}");
            }
        }
        Ok(())
    }
    .await;
    let cleanup = remove_directory(&staging, "download staging directory").await;
    if committed {
        if let Err(error) = cleanup {
            log::warn!("{error}");
        }
        return Ok(());
    }
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup_error)) => Err(format!("{error}; {cleanup_error}")),
    }
}

pub async fn download_pinned(
    client: &Client,
    root: &Path,
    operation_lock: &tokio::sync::Mutex<()>,
    gate: &tokio::sync::Mutex<()>,
    unload: impl std::future::Future<Output = ()>,
    cancel: Arc<AtomicBool>,
    progress: impl FnMut(u64, u64),
) -> Result<(), String> {
    download(
        DownloadRequest {
            client,
            root,
            manifest: &PINNED,
            operation_lock,
            gate,
            cancel,
            stall: STALL,
        },
        |name| format!("https://huggingface.co/{REPO}/resolve/{REVISION}/{name}"),
        unload,
        progress,
    )
    .await
}

/// Serialize against download, then hold the shared model gate only while
/// unloading and removing the active model.
pub async fn delete(
    operation_lock: &tokio::sync::Mutex<()>,
    gate: &tokio::sync::Mutex<()>,
    directory: &Path,
    unload: impl std::future::Future<Output = ()>,
) -> Result<(), String> {
    let _operation = operation_lock.lock().await;
    let root = directory
        .parent()
        .ok_or_else(|| "Cannot inspect the Parakeet model directory".to_string())?;
    reclaim_transactions(root, &PINNED).await?;
    {
        let _guard = gate.lock().await;
        unload.await;
        remove_directory(directory, "model directory").await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const TEST_FILES: [ModelFile; 3] = [
        ModelFile {
            name: "encoder-model.int8.onnx",
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        },
        ModelFile {
            name: "decoder_joint-model.int8.onnx",
            size: 3,
            sha256: "cb8379ac2098aa165029e3938a51da0bcecfc008fd6795f401178647f96c5b34",
        },
        ModelFile {
            name: "vocab.txt",
            size: 3,
            sha256: "50ae61e841fac4e8f9e40baf2ad36ec868922ea48368c18f9535e47db56dd7fb",
        },
    ];

    async fn write_complete_test_model(path: &Path) {
        fs::create_dir_all(path).await.unwrap();
        for (name, bytes) in [
            ("encoder-model.int8.onnx", b"abc".as_slice()),
            ("decoder_joint-model.int8.onnx", b"def".as_slice()),
            ("vocab.txt", b"ghi".as_slice()),
        ] {
            fs::write(path.join(name), bytes).await.unwrap();
        }
        fs::write(path.join(MARKER), "test-revision").await.unwrap();
    }

    #[tokio::test]
    async fn startup_recovery_restores_complete_backup_and_preserves_commits() {
        let manifest = Manifest {
            revision: "test-revision",
            files: &TEST_FILES,
        };
        for committed in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let destination = model_directory(temp.path());
            let staging = temp.path().join(format!(".staging-{MODEL_ID}-abandoned"));
            let backup = temp.path().join(format!(".replaced-{MODEL_ID}-abandoned"));
            fs::create_dir(&staging).await.unwrap();
            write_complete_test_model(&backup).await;
            if committed {
                write_complete_test_model(&destination).await;
                fs::write(destination.join("keep"), b"yes").await.unwrap();
            } else {
                fs::create_dir(&destination).await.unwrap();
                fs::write(destination.join("partial"), b"bad")
                    .await
                    .unwrap();
            }
            reclaim_transactions(temp.path(), &manifest).await.unwrap();
            assert!(is_downloaded(&destination, "test-revision", &TEST_FILES));
            assert!(!staging.exists());
            assert!(!backup.exists());
            assert_eq!(destination.join("keep").exists(), committed);
            assert!(!destination.join("partial").exists());
        }
    }

    #[tokio::test]
    async fn startup_recovery_removes_uncommitted_without_complete_backup() {
        let temp = tempfile::tempdir().unwrap();
        let destination = model_directory(temp.path());
        let backup = temp.path().join(format!(".replaced-{MODEL_ID}-incomplete"));
        fs::create_dir(&destination).await.unwrap();
        fs::create_dir(&backup).await.unwrap();
        reclaim_transactions(
            temp.path(),
            &Manifest {
                revision: "test-revision",
                files: &TEST_FILES,
            },
        )
        .await
        .unwrap();
        assert!(!destination.exists());
        assert!(!backup.exists());
    }

    async fn server(fail: Option<(&'static str, u16)>, stall: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut request = [0u8; 2048];
                    let count = socket.read(&mut request).await.unwrap_or(0);
                    let request = String::from_utf8_lossy(&request[..count]);
                    let name = request.split_whitespace().nth(1).unwrap_or("");
                    let (status, body) = if let Some((_, status)) =
                        fail.filter(|(failed, _)| name.ends_with(failed))
                    {
                        (status, b"error".as_slice())
                    } else if name.ends_with("encoder-model.int8.onnx") {
                        (200, b"abc".as_slice())
                    } else if name.ends_with("decoder_joint-model.int8.onnx") {
                        (200, b"def".as_slice())
                    } else {
                        (200, b"ghi".as_slice())
                    };
                    let header = format!(
                        "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    if socket.write_all(header.as_bytes()).await.is_err() {
                        return;
                    }
                    if stall {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    } else {
                        let _ = socket.write_all(body).await;
                    }
                });
            }
        });
        url
    }

    async fn run(
        root: &Path,
        url: &str,
        files: &[ModelFile],
        cancel: Arc<AtomicBool>,
        stall: Duration,
        progress: impl FnMut(u64, u64),
    ) -> Result<(), String> {
        let operation_lock = tokio::sync::Mutex::new(());
        let gate = tokio::sync::Mutex::new(());
        download(
            DownloadRequest {
                client: &Client::new(),
                root,
                manifest: &Manifest {
                    revision: "test-revision",
                    files,
                },
                operation_lock: &operation_lock,
                gate: &gate,
                cancel,
                stall,
            },
            |name| format!("{url}/{name}"),
            async {},
            progress,
        )
        .await
    }

    fn staging_empty(root: &Path) -> bool {
        std::fs::read_dir(root)
            .unwrap()
            .flatten()
            .all(|entry| !entry.file_name().to_string_lossy().starts_with(".staging-"))
    }

    #[test]
    fn pinned_manifest_is_well_formed() {
        assert_eq!(REVISION.len(), 40);
        assert!(REVISION.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(total_size(&FILES), 670_479_942);
        for file in FILES {
            assert!(file.size > 0);
            assert_eq!(file.sha256.len(), 64);
            assert!(file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert!(!file.name.contains('/'));
        }
    }

    #[tokio::test]
    async fn success_marker_last_and_corrupt_marker_redownloads() {
        let temp = tempfile::tempdir().unwrap();
        let url = server(None, false).await;
        let destination = model_directory(temp.path());
        let mut progress = Vec::new();
        run(
            temp.path(),
            &url,
            &TEST_FILES,
            Arc::new(AtomicBool::new(false)),
            STALL,
            |done, total| {
                assert!(!destination.join(MARKER).exists());
                progress.push((done, total));
            },
        )
        .await
        .unwrap();
        assert_eq!(progress.last(), Some(&(9, 9)));
        assert!(is_downloaded(&destination, "test-revision", &TEST_FILES));
        assert!(staging_empty(temp.path()));
        fs::write(destination.join(TEST_FILES[0].name), b"short")
            .await
            .unwrap();
        assert!(!is_downloaded(&destination, "test-revision", &TEST_FILES));
        fs::write(destination.join(MARKER), b"corrupt")
            .await
            .unwrap();
        assert!(!is_downloaded(&destination, "test-revision", &TEST_FILES));
        run(
            temp.path(),
            &url,
            &TEST_FILES,
            Arc::new(AtomicBool::new(false)),
            STALL,
            |_, _| {},
        )
        .await
        .unwrap();
        assert!(is_downloaded(&destination, "test-revision", &TEST_FILES));
        assert!(staging_empty(temp.path()));
    }

    #[tokio::test]
    async fn wrong_size_hash_and_http_errors_leave_no_install_or_staging() {
        let url = server(None, false).await;
        let mut wrong_size = TEST_FILES;
        wrong_size[1].size = 4;
        let mut wrong_hash = TEST_FILES;
        wrong_hash[1].sha256 = TEST_FILES[0].sha256;
        for (base, files, expected) in [
            (&url, &wrong_size[..], "wrong size"),
            (&url, &wrong_hash[..], "verification"),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let error = run(
                temp.path(),
                base,
                files,
                Arc::new(AtomicBool::new(false)),
                STALL,
                |_, _| {},
            )
            .await
            .unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert!(!model_directory(temp.path()).exists());
            assert!(staging_empty(temp.path()));
        }
        for status in [404, 500] {
            let temp = tempfile::tempdir().unwrap();
            let url = server(Some(("decoder_joint-model.int8.onnx", status)), false).await;
            let error = run(
                temp.path(),
                &url,
                &TEST_FILES,
                Arc::new(AtomicBool::new(false)),
                STALL,
                |_, _| {},
            )
            .await
            .unwrap_err();
            assert!(error.contains(&format!("HTTP {status}")));
            assert!(!model_directory(temp.path()).exists());
            assert!(staging_empty(temp.path()));
        }
    }

    #[tokio::test]
    async fn stalled_and_user_cancelled_downloads_clean_partial_staging() {
        let url = server(None, true).await;
        let temp = tempfile::tempdir().unwrap();
        let error = run(
            temp.path(),
            &url,
            &TEST_FILES,
            Arc::new(AtomicBool::new(false)),
            Duration::from_millis(80),
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(error.contains("stalled"));
        assert!(staging_empty(temp.path()));
        let cancel = Arc::new(AtomicBool::new(false));
        let to_cancel = Arc::clone(&cancel);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            to_cancel.store(true, Ordering::SeqCst);
        });
        let error = run(temp.path(), &url, &TEST_FILES, cancel, STALL, |_, _| {})
            .await
            .unwrap_err();
        assert!(error.contains("cancelled"));
        assert!(staging_empty(temp.path()));
    }

    #[tokio::test]
    async fn slow_download_does_not_block_a_transcription_gate_user() {
        let temp = tempfile::tempdir().unwrap();
        let url = server(None, true).await;
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let operation = Arc::new(tokio::sync::Mutex::new(()));
        let cancel = Arc::new(AtomicBool::new(false));
        let root = temp.path().to_path_buf();
        let task_cancel = Arc::clone(&cancel);
        let task_gate = Arc::clone(&gate);
        let task_operation = Arc::clone(&operation);
        let task = tokio::spawn(async move {
            download(
                DownloadRequest {
                    client: &Client::new(),
                    root: &root,
                    manifest: &Manifest {
                        revision: "test-revision",
                        files: &TEST_FILES,
                    },
                    operation_lock: &task_operation,
                    gate: &task_gate,
                    cancel: task_cancel,
                    stall: STALL,
                },
                |name| format!("{url}/{name}"),
                async {},
                |_, _| {},
            )
            .await
        });
        for _ in 0..100 {
            if !staging_empty(temp.path()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!staging_empty(temp.path()));
        assert!(!task.is_finished());
        let transcription_guard = tokio::time::timeout(Duration::from_millis(200), gate.lock())
            .await
            .expect("transcription must acquire the shared gate during network transfer");
        drop(transcription_guard);
        cancel.store(true, Ordering::SeqCst);
        assert!(task.await.unwrap().unwrap_err().contains("cancelled"));
    }

    #[tokio::test]
    async fn reclaims_owned_transactions_before_disk_space_check() {
        let temp = tempfile::tempdir().unwrap();
        for name in [
            format!(".staging-{MODEL_ID}-abandoned"),
            format!(".replaced-{MODEL_ID}-abandoned"),
        ] {
            fs::create_dir(temp.path().join(name)).await.unwrap();
        }
        let unrelated = temp.path().join(".staging-other-model-abandoned");
        fs::create_dir(&unrelated).await.unwrap();
        let huge = [ModelFile {
            name: "huge.onnx",
            size: u64::MAX / 4,
            sha256: "unused",
        }];
        let error = run(
            temp.path(),
            "http://127.0.0.1:1",
            &huge,
            Arc::new(AtomicBool::new(false)),
            STALL,
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(error.contains("free disk space"), "{error}");
        assert!(std::fs::read_dir(temp.path())
            .unwrap()
            .flatten()
            .all(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                !name.starts_with(&format!(".staging-{MODEL_ID}-"))
                    && !name.starts_with(&format!(".replaced-{MODEL_ID}-"))
            }));
        assert!(unrelated.exists());
    }

    #[tokio::test]
    async fn failed_transaction_removal_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(
            temp.path().join(format!(".staging-{MODEL_ID}-blocked")),
            b"file",
        )
        .await
        .unwrap();
        let error = run(
            temp.path(),
            "http://127.0.0.1:1",
            &TEST_FILES,
            Arc::new(AtomicBool::new(false)),
            STALL,
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(error.contains("Cannot remove the Parakeet leftover download transaction"));
    }

    #[tokio::test]
    async fn cancellation_before_marker_commit_leaves_no_download() {
        let temp = tempfile::tempdir().unwrap();
        let url = server(None, false).await;
        let cancel = Arc::new(AtomicBool::new(false));
        let hook_cancel = Arc::clone(&cancel);
        let error = download_with_hook(
            DownloadRequest {
                client: &Client::new(),
                root: temp.path(),
                manifest: &Manifest {
                    revision: "test-revision",
                    files: &TEST_FILES,
                },
                operation_lock: &tokio::sync::Mutex::new(()),
                gate: &tokio::sync::Mutex::new(()),
                cancel,
                stall: STALL,
            },
            |name| format!("{url}/{name}"),
            async {},
            |_, _| {},
            |point| {
                if matches!(point, PublishPoint::BeforeCommit) {
                    hook_cancel.store(true, Ordering::SeqCst);
                }
            },
        )
        .await
        .unwrap_err();
        assert!(error.contains("cancelled"), "{error}");
        assert!(!is_downloaded(
            &model_directory(temp.path()),
            "test-revision",
            &TEST_FILES
        ));
    }

    #[tokio::test]
    async fn cancellation_after_marker_commit_reports_success() {
        let temp = tempfile::tempdir().unwrap();
        let url = server(None, false).await;
        let cancel = Arc::new(AtomicBool::new(false));
        let hook_cancel = Arc::clone(&cancel);
        download_with_hook(
            DownloadRequest {
                client: &Client::new(),
                root: temp.path(),
                manifest: &Manifest {
                    revision: "test-revision",
                    files: &TEST_FILES,
                },
                operation_lock: &tokio::sync::Mutex::new(()),
                gate: &tokio::sync::Mutex::new(()),
                cancel,
                stall: STALL,
            },
            |name| format!("{url}/{name}"),
            async {},
            |_, _| {},
            |point| {
                if matches!(point, PublishPoint::AfterCommit) {
                    hook_cancel.store(true, Ordering::SeqCst);
                }
            },
        )
        .await
        .unwrap();
        assert!(is_downloaded(
            &model_directory(temp.path()),
            "test-revision",
            &TEST_FILES
        ));
    }

    #[tokio::test]
    async fn failed_rollback_is_explicit_and_leaves_no_committed_download() {
        let temp = tempfile::tempdir().unwrap();
        let destination = model_directory(temp.path());
        fs::create_dir(&destination).await.unwrap();
        fs::write(destination.join("old"), b"old").await.unwrap();
        fs::write(destination.join(MARKER), "old-revision")
            .await
            .unwrap();
        let url = server(None, false).await;
        let cancel = Arc::new(AtomicBool::new(false));
        let hook_cancel = Arc::clone(&cancel);
        let root = temp.path().to_path_buf();
        let error = download_with_hook(
            DownloadRequest {
                client: &Client::new(),
                root: temp.path(),
                manifest: &Manifest {
                    revision: "test-revision",
                    files: &TEST_FILES,
                },
                operation_lock: &tokio::sync::Mutex::new(()),
                gate: &tokio::sync::Mutex::new(()),
                cancel,
                stall: STALL,
            },
            |name| format!("{url}/{name}"),
            async {},
            |_, _| {},
            |point| match point {
                PublishPoint::BeforeCommit => hook_cancel.store(true, Ordering::SeqCst),
                PublishPoint::BeforeRollback => {
                    let backup = std::fs::read_dir(&root)
                        .unwrap()
                        .flatten()
                        .find(|entry| {
                            entry
                                .file_name()
                                .to_string_lossy()
                                .starts_with(&format!(".replaced-{MODEL_ID}-"))
                        })
                        .unwrap();
                    std::fs::remove_dir_all(backup.path()).unwrap();
                }
                PublishPoint::AfterCommit => {}
            },
        )
        .await
        .unwrap_err();
        assert!(error.contains("Rollback failed"), "{error}");
        assert!(!is_downloaded(&destination, "test-revision", &TEST_FILES));
    }

    #[tokio::test]
    async fn delete_waits_for_gate_and_unloads_before_removing_files() {
        let temp = tempfile::tempdir().unwrap();
        let directory = model_directory(temp.path());
        fs::create_dir(&directory).await.unwrap();
        fs::write(directory.join(MARKER), REVISION).await.unwrap();
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let operation_lock = Arc::new(tokio::sync::Mutex::new(()));
        let held = gate.lock().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let gate_for_delete = Arc::clone(&gate);
        let operation_for_delete = Arc::clone(&operation_lock);
        let calls_for_delete = Arc::clone(&calls);
        let directory_for_delete = directory.clone();
        let task = tokio::spawn(async move {
            delete(
                &operation_for_delete,
                &gate_for_delete,
                &directory_for_delete,
                async {
                    assert!(directory_for_delete.exists());
                    calls_for_delete.fetch_add(1, Ordering::SeqCst);
                },
            )
            .await
            .unwrap();
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(directory.exists());
        drop(held);
        task.await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!directory.exists());
    }
}
