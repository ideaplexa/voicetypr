//! Verify while writing, then publish atomically. This avoids a second read of
//! multi-gigabyte weights and never exposes a partial file as an installed model.
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub async fn download_file(
    url: &str,
    path: &Path,
    size: u64,
    sha256: &str,
    cancelled: &Arc<AtomicBool>,
    progress: impl Fn(u64, u64),
) -> Result<(), String> {
    let partial = path.with_extension("partial");
    let operation = async {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| "Unable to start model download")?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|_| "Model download failed")?
            .error_for_status()
            .map_err(|_| "Model download server returned an error")?;
        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&partial)
            .await
            .map_err(|_| "Unable to create model download")?;
        let mut hash = Sha256::new();
        let mut downloaded = 0u64;
        let mut reported = 0;
        while let Some(chunk) = tokio::time::timeout(Duration::from_secs(90), stream.next())
            .await
            .map_err(|_| "Model download timed out")?
        {
            let chunk = chunk.map_err(|_| "Model download interrupted")?;
            downloaded += chunk.len() as u64;
            if downloaded > size {
                return Err("Model download exceeded expected size".to_string());
            }
            hash.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(|_| "Unable to write model download")?;
            if downloaded.saturating_sub(reported) >= (size / 100).max(1) {
                progress(downloaded, size);
                reported = downloaded;
            }
        }
        if downloaded != size || hex::encode(hash.finalize()) != sha256 {
            return Err("Model checksum verification failed".to_string());
        }
        file.flush()
            .await
            .map_err(|_| "Unable to finish model download")?;
        file.sync_all()
            .await
            .map_err(|_| "Unable to finish model download")?;
        drop(file);
        if cancelled.load(Ordering::SeqCst) {
            return Err("Download cancelled by user".to_string());
        }
        tokio::fs::rename(&partial, path)
            .await
            .map_err(|_| "Unable to install verified model")?;
        progress(size, size);
        Ok(())
    };
    let result = tokio::select! {
        result = operation => result,
        _ = async { while !cancelled.load(Ordering::SeqCst) { tokio::time::sleep(Duration::from_millis(20)).await; } } => Err("Download cancelled by user".into()),
    };
    if result.is_err() {
        let _ = tokio::fs::remove_file(partial).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    #[tokio::test]
    async fn cancellation_preserves_the_installed_model_and_removes_partial_state() {
        let server = MockServer::start().await;
        Mock::given(path("/weights"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        let model = temp.path().join("model.gguf");
        std::fs::write(&model, b"original").unwrap();
        std::fs::write(model.with_extension("partial"), b"incomplete").unwrap();
        let cancelled = Arc::new(AtomicBool::new(true));
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            download_file(
                &format!("{}/weights", server.uri()),
                &model,
                8,
                &"0".repeat(64),
                &cancelled,
                |_, _| {},
            ),
        )
        .await
        .unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read(&model).unwrap(), b"original");
        assert!(!model.with_extension("partial").exists());
    }

    #[tokio::test]
    async fn checksum_failure_never_installs_a_partial_model() {
        let server = MockServer::start().await;
        Mock::given(path("/weights"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"wrong"))
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        let model = temp.path().join("model.gguf");
        let result = download_file(
            &format!("{}/weights", server.uri()),
            &model,
            5,
            &"0".repeat(64),
            &Arc::new(AtomicBool::new(false)),
            |_, _| {},
        )
        .await;
        assert!(result.is_err());
        assert!(!model.exists());
        assert!(!model.with_extension("partial").exists());
    }
    #[tokio::test]
    async fn matching_bytes_are_installed_after_verification() {
        let server = MockServer::start().await;
        Mock::given(path("/weights"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"weights"))
            .mount(&server)
            .await;
        let temp = tempfile::tempdir().unwrap();
        let model = temp.path().join("model.gguf");
        download_file(
            &format!("{}/weights", server.uri()),
            &model,
            7,
            &hex::encode(Sha256::digest(b"weights")),
            &Arc::new(AtomicBool::new(false)),
            |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(model).unwrap(), b"weights");
    }
}
