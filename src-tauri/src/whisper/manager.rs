use futures_util::StreamExt;
use reqwest;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

// Type-safe size validation
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Field accessed through as_bytes() in tests
pub struct ModelSize(u64);

impl ModelSize {
    const MAX_MODEL_SIZE: u64 = 3584 * 1024 * 1024; // 3.5GB max
    const MIN_MODEL_SIZE: u64 = 10 * 1024 * 1024; // 10MB min (reasonable for smallest model)

    pub fn new(size: u64) -> Result<Self, String> {
        if size < Self::MIN_MODEL_SIZE {
            return Err(format!(
                "Model size {} bytes ({:.1}MB) is too small. Minimum size is 10MB",
                size,
                size as f64 / 1024.0 / 1024.0
            ));
        }
        if size > Self::MAX_MODEL_SIZE {
            return Err(format!(
                "Model size {} bytes ({:.1}GB) exceeds maximum allowed size of 3.5GB",
                size,
                size as f64 / 1024.0 / 1024.0 / 1024.0
            ));
        }
        Ok(ModelSize(size))
    }

    #[cfg(test)]
    pub fn as_bytes(&self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
    pub size: u64,
    pub url: String,
    pub sha256: String,
    pub downloaded: bool,
    pub speed_score: u8,    // 1-10, 10 being fastest
    pub accuracy_score: u8, // 1-10, 10 being most accurate
    pub recommended: bool,  // Whether this model is recommended
}

impl ModelInfo {
    /// Validate and get size as ModelSize
    pub fn validated_size(&self) -> Result<ModelSize, String> {
        ModelSize::new(self.size)
    }
}

pub struct WhisperManager {
    models_dir: PathBuf,
    models: HashMap<String, ModelInfo>,
}

/// Whether a catalog model is offered in the UI list. Pure so both platforms are testable.
pub(crate) fn is_model_listed(name: &str, downloaded: bool, is_macos: bool) -> bool {
    !(is_macos && name == "small.en" && !downloaded)
}

impl WhisperManager {
    /// Validate model name to prevent path traversal and ensure it's a known model
    fn is_valid_model_name(&self, model_name: &str) -> bool {
        // First check if it's a known model
        if !self.models.contains_key(model_name) {
            return false;
        }

        // Additional safety check for path traversal
        if model_name.contains('/') || model_name.contains('\\') || model_name.contains("..") {
            return false;
        }

        // Only allow alphanumeric, dash, underscore, and dot
        model_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    }

    pub fn new(models_dir: PathBuf) -> Self {
        let mut models = HashMap::new();

        // Define available models from the pinned Hugging Face whisper.cpp snapshot.
        // URLs are pinned to an immutable revision so catalog size and checksum metadata
        // remain stable across releases.

        // HF LFS SHA256 checksums; field name retained for API compatibility.

        models.insert(
            "base.en".to_string(),
            ModelInfo {
                name: "base.en".to_string(),
                display_name: "Base (English)".to_string(),
                size: 147_964_211,
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.en.bin"
                    .to_string(),
                sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"
                    .to_string(),
                downloaded: false,
                speed_score: 8,    // Very fast
                accuracy_score: 5, // Basic accuracy
                recommended: false,
            },
        );

        models.insert(
            "large-v3".to_string(),
            ModelInfo {
                name: "large-v3".to_string(),
                display_name: "Large v3".to_string(),
                size: 3_095_033_483,
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3.bin"
                    .to_string(),
                sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2"
                    .to_string(),
                downloaded: false,
                speed_score: 2,    // Slowest
                accuracy_score: 9, // Best accuracy
                recommended: true, // Recommended model
            },
        );

        // Removed: large-v3-q5_0 to simplify model list

        models.insert(
            "large-v3-turbo".to_string(),
            ModelInfo {
                name: "large-v3-turbo".to_string(),
                display_name: "Large v3 Turbo".to_string(),
                size: 1_624_555_275,
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3-turbo.bin"
                    .to_string(),
                sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"
                    .to_string(),
                downloaded: false,
                speed_score: 7,    // 6x faster than large-v3
                accuracy_score: 9, // Comparable to large-v2
                recommended: true, // Recommended model
            },
        );

        models.insert(
            "small.en".to_string(),
            ModelInfo {
                name: "small.en".to_string(),
                display_name: "Small (English)".to_string(),
                size: 487_614_201,
                url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.en.bin"
                    .to_string(),
                sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d"
                    .to_string(),
                downloaded: false,
                speed_score: 7,    // Fast for English-only
                accuracy_score: 6, // Good accuracy for English
                recommended: false,
            },
        );

        // Removed: large-v3-turbo-q8_0 to simplify model list

        let mut manager = Self { models_dir, models };
        manager.check_downloaded_models();
        manager
    }

    fn check_downloaded_models(&mut self) {
        // Check each known model directly instead of scanning directory
        for (model_name, model_info) in self.models.iter_mut() {
            let model_path = self.models_dir.join(format!("{}.bin", model_name));
            let exists = model_path.exists();

            if exists {
                // Double-check with metadata to ensure file is actually accessible
                if let Ok(metadata) = std::fs::metadata(&model_path) {
                    let file_size = metadata.len();
                    let expected_size = model_info.size;

                    if file_size == expected_size {
                        model_info.downloaded = true;
                    } else if file_size > 0 {
                        log::warn!("[check_downloaded_models] Model '{}' exists but has wrong size: {} bytes (expected: {} bytes)",
                            model_name, file_size, expected_size);
                        model_info.downloaded = false;
                    } else {
                        log::warn!(
                            "[check_downloaded_models] Model '{}' exists but has 0 bytes",
                            model_name
                        );
                        model_info.downloaded = false;
                    }
                } else {
                    log::warn!(
                        "[check_downloaded_models] Model '{}' path exists but cannot read metadata",
                        model_name
                    );
                    model_info.downloaded = false;
                }
            } else {
                model_info.downloaded = false;
            }
        }
    }

    pub fn models_dir(&self) -> PathBuf {
        self.models_dir.clone()
    }

    /// Get model info needed for download (doesn't hold lock during download)
    pub fn get_model_info(&self, model_name: &str) -> Result<(ModelInfo, PathBuf), String> {
        // Use centralized validation
        if !self.is_valid_model_name(model_name) {
            return Err(format!("Invalid model name: '{}'", model_name));
        }

        let model = self.models.get(model_name).ok_or(format!(
            "Model '{}' not found in available models",
            model_name
        ))?;

        // Validate model size before downloading
        let _ = model.validated_size()?;

        let output_path = self.models_dir.join(format!("{}.bin", model_name));

        Ok((model.clone(), output_path))
    }

    /// Download a model file (should be called without holding the manager lock)
    pub async fn download_model_file(
        model_info: &ModelInfo,
        output_path: &PathBuf,
        models_dir: &PathBuf,
        cancel_flag: Option<Arc<AtomicBool>>,
        progress_callback: impl Fn(u64, u64),
    ) -> Result<(), String> {
        log::info!("Downloading model {}", model_info.name);

        log::debug!("Model URL: {}", model_info.url);
        log::debug!("Model size: {} bytes", model_info.size);

        // Create models directory if it doesn't exist
        fs::create_dir_all(models_dir)
            .await
            .map_err(|e| format!("Failed to create models directory: {}", e))?;

        // Check if the file already exists. Exact-size files are already present;
        // any nonzero size mismatch is stale/corrupt and is removed only because
        // this path runs from an explicit user-initiated download/repair action.
        if output_path.exists() {
            if let Ok(metadata) = fs::metadata(&output_path).await {
                let file_size = metadata.len();
                let expected_size = model_info.size;

                if file_size == expected_size {
                    log::info!(
                        "Model '{}' already exists with exact expected size. Verifying checksum before accepting.",
                        model_info.name
                    );
                    Self::verify_model_checksum(model_info, output_path).await?;
                    return Ok(());
                }

                if file_size > 0 {
                    log::warn!(
                        "Found stale/corrupt model file for '{}': {} bytes (expected: {} bytes). Removing before re-download...",
                        model_info.name, file_size, expected_size
                    );

                    if let Err(e) = fs::remove_file(&output_path).await {
                        log::error!("Failed to remove stale/corrupt model file: {}", e);
                        return Err(format!("Failed to remove stale/corrupt model file: {}", e));
                    }

                    log::info!("Stale/corrupt model file removed successfully");
                }
            }
        }

        log::info!("[download_model] Output path: {:?}", output_path);
        log::info!(
            "[download_model] File name will be: {}.bin",
            model_info.name
        );

        // Download the model
        let client = reqwest::Client::new();
        let response = client
            .get(&model_info.url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let total_size = response.content_length().unwrap_or(model_info.size);

        // Validate reported size matches expected size (allow 10% variance for compression)
        let size_variance =
            (total_size as f64 - model_info.size as f64).abs() / model_info.size as f64;
        if size_variance > 0.1 {
            return Err(format!(
                "Model size mismatch: expected {} bytes, server reports {} bytes ({}% difference)",
                model_info.size,
                total_size,
                (size_variance * 100.0) as u32
            ));
        }

        // Validate the total size is within our limits
        let _ = ModelSize::new(total_size)?;

        let mut file = fs::File::create(&output_path)
            .await
            .map_err(|e| e.to_string())?;

        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        let mut last_progress_update = 0u64;
        let update_threshold = total_size / 100; // Update every 1%

        while let Some(chunk) = stream.next().await {
            // Check for cancellation
            if let Some(ref flag) = cancel_flag {
                if flag.load(Ordering::Relaxed) {
                    log::info!("Download cancelled by user for model: {}", model_info.name);
                    // Clean up partial download
                    drop(file);
                    let _ = fs::remove_file(&output_path).await;
                    return Err("Download cancelled by user".to_string());
                }
            }

            let chunk = chunk.map_err(|e| e.to_string())?;

            // Prevent downloading more than expected (with 1% tolerance)
            if downloaded + chunk.len() as u64 > (total_size as f64 * 1.01) as u64 {
                // Clean up partial download
                drop(file);
                let _ = fs::remove_file(&output_path).await;

                return Err(format!(
                    "Download exceeded expected size: downloaded {} bytes, expected {} bytes",
                    downloaded + chunk.len() as u64,
                    total_size
                ));
            }

            file.write_all(&chunk).await.map_err(|e| e.to_string())?;

            downloaded += chunk.len() as u64;

            // Only update progress every 1% to avoid flooding the UI
            if downloaded - last_progress_update >= update_threshold || downloaded == total_size {
                progress_callback(downloaded, total_size);
                last_progress_update = downloaded;
            }
        }

        // Ensure file is flushed to disk
        file.flush().await.map_err(|e| e.to_string())?;
        // Force OS to write to physical disk
        file.sync_all()
            .await
            .map_err(|e| format!("Failed to sync file to disk: {}", e))?;
        drop(file);

        // Also sync the parent directory to ensure directory entry is visible
        if let Some(parent) = output_path.parent() {
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }

        // Ensure final 100% progress is sent
        if downloaded < total_size {
            progress_callback(total_size, total_size);
        }

        Self::verify_model_checksum(model_info, output_path).await?;

        // Log what files are in the directory after download
        log::info!("[download_model] Download complete. Listing models directory:");
        if let Ok(entries) = std::fs::read_dir(models_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    log::info!("[download_model]   Found file: {}", name);
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn verify_model_checksum(
        model_info: &ModelInfo,
        output_path: &PathBuf,
    ) -> Result<(), String> {
        if !model_info.sha256.is_empty() {
            log::info!("Verifying model checksum...");
            match model_info.sha256.len() {
                40 => {
                    // SHA1 checksum (legacy from whisper.cpp)
                    Self::verify_sha1_checksum(output_path, &model_info.sha256).await?;
                }
                64 => {
                    // SHA256 checksum (preferred)
                    Self::verify_sha256_checksum(output_path, &model_info.sha256).await?;
                }
                _ => {
                    log::warn!(
                        "Invalid checksum length for {}. Skipping verification.",
                        model_info.name
                    );
                    log::warn!(
                        "Expected SHA1 (40 chars) or SHA256 (64 chars), got {} chars.",
                        model_info.sha256.len()
                    );
                }
            }
        } else {
            log::warn!(
                "No checksum available for {}. Skipping verification.",
                model_info.name
            );
            log::warn!("File integrity cannot be guaranteed without checksum verification.");
        }

        Ok(())
    }

    /// Verify the SHA256 checksum of a downloaded file
    async fn verify_sha256_checksum(
        file_path: &PathBuf,
        expected_checksum: &str,
    ) -> Result<(), String> {
        // Open the file
        let mut file = fs::File::open(file_path)
            .await
            .map_err(|e| format!("Failed to open file for checksum verification: {}", e))?;

        // Read file in chunks and calculate hash
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; 8192]; // 8KB buffer

        loop {
            let bytes_read = file
                .read(&mut buffer)
                .await
                .map_err(|e| format!("Failed to read file for checksum: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        // Get the final hash
        let result = hasher.finalize();
        let calculated_checksum = format!("{:x}", result);

        // Compare checksums
        if calculated_checksum != expected_checksum {
            // Delete the corrupted file
            let _ = fs::remove_file(file_path).await;
            return Err(format!(
                "Checksum verification failed!\nExpected: {}\nCalculated: {}\nFile has been deleted.",
                expected_checksum,
                calculated_checksum
            ));
        }

        log::info!("SHA256 checksum verified successfully!");
        Ok(())
    }

    /// Verify the SHA1 checksum of a downloaded file (legacy support for whisper.cpp models)
    async fn verify_sha1_checksum(
        file_path: &PathBuf,
        expected_checksum: &str,
    ) -> Result<(), String> {
        // Open the file
        let mut file = fs::File::open(file_path)
            .await
            .map_err(|e| format!("Failed to open file for checksum verification: {}", e))?;

        // Read file in chunks and calculate hash
        let mut hasher = Sha1::new();
        let mut buffer = vec![0; 8192]; // 8KB buffer

        loop {
            let bytes_read = file
                .read(&mut buffer)
                .await
                .map_err(|e| format!("Failed to read file for checksum: {}", e))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        // Get the final hash
        let result = hasher.finalize();
        let calculated_checksum = format!("{:x}", result);

        // Compare checksums
        if calculated_checksum != expected_checksum {
            // Delete the corrupted file
            let _ = fs::remove_file(file_path).await;
            return Err(format!(
                "SHA1 checksum verification failed!\nExpected: {}\nCalculated: {}\nFile has been deleted.",
                expected_checksum,
                calculated_checksum
            ));
        }

        log::info!("SHA1 checksum verified successfully!");
        Ok(())
    }

    pub fn get_model_path(&self, model_name: &str) -> Option<PathBuf> {
        // Use centralized validation
        if !self.is_valid_model_name(model_name) {
            log::warn!("Rejected invalid model name: {}", model_name);
            return None;
        }

        if self.models.get(model_name)?.downloaded {
            Some(self.models_dir.join(format!("{}.bin", model_name)))
        } else {
            None
        }
    }

    pub fn get_models_status(&self) -> HashMap<String, ModelInfo> {
        self.models.clone()
    }

    /// Catalog entries to show in the model list. On macOS, Whisper small.en is
    /// hidden unless already downloaded (Parakeet covers that tier); elsewhere
    /// every entry is listed.
    pub fn get_listed_models(&self) -> HashMap<String, ModelInfo> {
        self.models
            .iter()
            .filter(|(name, info)| {
                is_model_listed(name, info.downloaded, cfg!(target_os = "macos"))
            })
            .map(|(name, info)| (name.clone(), info.clone()))
            .collect()
    }

    pub fn get_models_status_mut(&mut self) -> &mut HashMap<String, ModelInfo> {
        &mut self.models
    }

    /// Check if any models are downloaded (efficient, no cloning)
    pub fn has_downloaded_models(&self) -> bool {
        self.models.values().any(|info| info.downloaded)
    }

    /// Get the name of the first downloaded model (for auto-selection)
    pub fn get_first_downloaded_model(&self) -> Option<String> {
        // Prefer recommended models first, then by speed score
        self.models
            .values()
            .filter(|info| info.downloaded)
            .max_by_key(|info| (info.recommended as u8, info.speed_score))
            .map(|info| info.name.clone())
    }

    /// Get list of downloaded model names (efficient, minimal allocation)
    pub fn get_downloaded_model_names(&self) -> Vec<String> {
        self.models
            .iter()
            .filter(|(_, info)| info.downloaded)
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn refresh_downloaded_status(&mut self) {
        // Reset all to not downloaded
        for model in self.models.values_mut() {
            model.downloaded = false;
        }

        // Check again
        self.check_downloaded_models();

        let n_downloaded = self.models.values().filter(|m| m.downloaded).count();
        log::debug!(
            "whisper model status: {}/{} downloaded",
            n_downloaded,
            self.models.len()
        );
    }

    /// Returns the names of downloaded model files that are recognized by the manager.
    /// Status is based on the exact-size checks populated by refresh/check, not existence alone.
    pub fn list_downloaded_files(&self) -> Vec<String> {
        self.models
            .iter()
            .filter(|(_, info)| info.downloaded)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Delete a model file from disk and refresh the downloaded status map.
    pub fn delete_model_file(&mut self, model_name: &str) -> Result<(), String> {
        // Use centralized validation
        if !self.is_valid_model_name(model_name) {
            return Err(format!("Invalid model name: '{}'", model_name));
        }

        let path = self.models_dir.join(format!("{}.bin", model_name));
        if !path.exists() {
            return Err("Model file not found".to_string());
        }
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;

        // update internal flags
        if let Some(info) = self.models.get_mut(model_name) {
            info.downloaded = false;
        }
        // Also refresh to catch any other changes
        self.refresh_downloaded_status();
        Ok(())
    }

    /// Calculate a balanced performance score (combines speed and accuracy)
    #[allow(dead_code)]
    pub fn calculate_balanced_score(speed: u8, accuracy: u8) -> f32 {
        // Weighted average: 40% speed, 60% accuracy
        // Most users want accuracy but also care about speed
        (speed as f32 * 0.4 + accuracy as f32 * 0.6) / 10.0 * 100.0
    }

    /// Get model names ordered by size (smallest to largest)
    pub fn get_models_by_size(&self) -> Vec<String> {
        let mut models: Vec<(&String, &ModelInfo)> = self.models.iter().collect();
        models.sort_by_key(|(_, info)| info.size);
        models.into_iter().map(|(name, _)| name.clone()).collect()
    }

    /// Get models sorted by a specific metric
    #[allow(dead_code)]
    pub fn get_models_sorted(&self, sort_by: &str) -> Vec<(String, ModelInfo)> {
        let mut models: Vec<(String, ModelInfo)> = self
            .models
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        match sort_by {
            "speed" => {
                models.sort_by_key(|(_, info)| std::cmp::Reverse(info.speed_score));
            }
            "accuracy" => {
                models.sort_by_key(|(_, info)| std::cmp::Reverse(info.accuracy_score));
            }
            "balanced" => {
                models.sort_by(|a, b| {
                    let score_a =
                        Self::calculate_balanced_score(a.1.speed_score, a.1.accuracy_score);
                    let score_b =
                        Self::calculate_balanced_score(b.1.speed_score, b.1.accuracy_score);
                    score_b
                        .partial_cmp(&score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            "size" => {
                models.sort_by_key(|(_, info)| info.size);
            }
            _ => {
                // Default: sort by balanced score
                models.sort_by(|a, b| {
                    let score_a =
                        Self::calculate_balanced_score(a.1.speed_score, a.1.accuracy_score);
                    let score_b =
                        Self::calculate_balanced_score(b.1.speed_score, b.1.accuracy_score);
                    score_b
                        .partial_cmp(&score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        models
    }

    /// Clear all downloaded models and reset the manager state
    pub fn clear_all(&mut self) {
        // This resets the manager to a fresh state
        // The actual deletion of model files is handled by the reset command
        log::info!("Clearing WhisperManager state");
    }

    #[cfg(test)]
    pub fn new_for_test(models_dir: PathBuf) -> Self {
        let mut models = HashMap::new();

        // Create test models with small sizes
        models.insert(
            "base.en".to_string(),
            ModelInfo {
                name: "base.en".to_string(),
                display_name: "Base (English)".to_string(),
                size: 1024, // 1KB for tests
                url: "https://test.example.com/base.en.bin".to_string(),
                sha256: "test_hash".to_string(),
                downloaded: false,
                speed_score: 8,
                accuracy_score: 5,
                recommended: false,
            },
        );

        models.insert(
            "large-v3".to_string(),
            ModelInfo {
                name: "large-v3".to_string(),
                display_name: "Large v3".to_string(),
                size: 2048, // 2KB for tests
                url: "https://test.example.com/large-v3.bin".to_string(),
                sha256: "test_hash_v3".to_string(),
                downloaded: false,
                speed_score: 2,
                accuracy_score: 9,
                recommended: true,
            },
        );

        let mut manager = Self { models, models_dir };
        manager.check_downloaded_models();
        manager
    }
}

#[cfg(test)]
mod listing_tests {
    use super::*;

    #[test]
    fn macos_hides_small_en_unless_downloaded() {
        assert!(!is_model_listed("small.en", false, true));
        assert!(is_model_listed("small.en", true, true));
        assert!(is_model_listed("base.en", false, true));
    }

    #[test]
    fn windows_always_lists_small_en() {
        assert!(is_model_listed("small.en", false, false));
        assert!(is_model_listed("small.en", true, false));
    }

    #[test]
    fn listed_models_follow_platform_for_undownloaded_small_en() {
        let dir = tempfile::tempdir().unwrap();
        let manager = WhisperManager::new(dir.path().to_path_buf());
        let listed = manager.get_listed_models();
        assert_eq!(listed.contains_key("small.en"), !cfg!(target_os = "macos"));
        assert!(manager.get_models_status().contains_key("small.en"));
    }
}
