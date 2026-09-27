use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;

use super::transcriber::Transcriber;
use crate::utils::logger::*;

/// Maximum number of models to keep in cache
/// Only cache the current model to minimize RAM usage (1-3GB per model)
const MAX_CACHE_SIZE: usize = 1;

/// Simple LRU cache that keeps loaded `Transcriber` models with size limits.
///
/// Loading a GGML model from disk can take hundreds of milliseconds and a lot
/// of RAM (1-3GB per model). By keeping a limited number of models in memory
/// we balance performance with memory usage.
pub struct TranscriberCache {
    /// Keyed by absolute path to the `.bin` model file plus context-load flags.
    map: HashMap<String, Arc<Transcriber>>,
    /// Track access order for LRU eviction
    lru_order: VecDeque<String>,
    /// Maximum number of models to cache
    max_size: usize,
}

impl Default for TranscriberCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TranscriberCache {
    /// Create an empty cache with default size limit.
    pub fn new() -> Self {
        Self::with_capacity(MAX_CACHE_SIZE)
    }

    /// Create a cache with a specific capacity.
    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            map: HashMap::new(),
            lru_order: VecDeque::new(),
            max_size: max_size.max(1), // At least 1
        }
    }

    /// Retrieve a cached transcriber, or load and cache it if it isn't present yet.
    /// Return an already-loaded transcriber for this model, or `None` if it is not
    /// currently cached — WITHOUT loading it. The live-preview factory uses this so it
    /// never blocks recorder start on a cold model load; a miss just means no preview
    /// for this recording (best effort). Same cache key as `get_or_create`.
    pub fn get_loaded(&mut self, model_path: &Path, speed_mode: bool) -> Option<Arc<Transcriber>> {
        let path = model_path.to_string_lossy();
        let key = format!("{path}|fa={speed_mode}");
        if self.map.contains_key(&key) {
            let transcriber = self.map.get(&key).cloned();
            self.update_lru(&key);
            return transcriber;
        }
        None
    }

    pub fn get_or_create(
        &mut self,
        model_path: &Path,
        speed_mode: bool,
    ) -> Result<Arc<Transcriber>, String> {
        log::info!(
            "[TRANSCRIPTION_DEBUG] get_or_create called with path: {:?}",
            model_path
        );

        // Check if the model file exists
        if !model_path.exists() {
            let error = format!("Model file does not exist: {:?}", model_path);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);
            return Err(error);
        }

        // Whisper context parameters are fixed at model-load time, so speed
        // mode must be part of the cache key.
        let path = model_path.to_string_lossy();
        let key = format!("{path}|fa={speed_mode}");

        // Check if already cached
        if self.map.contains_key(&key) {
            log::info!("[TRANSCRIPTION_DEBUG] Model found in cache: {}", key);
            // Clone the transcriber before updating LRU
            let transcriber = self.map.get(&key).cloned();
            // Move to end of LRU order
            self.update_lru(&key);
            if let Some(t) = transcriber {
                return Ok(t);
            }
        }

        // Not cached – check if we need to evict
        if self.map.len() >= self.max_size {
            log::info!("[TRANSCRIPTION_DEBUG] Cache full, evicting LRU model");
            self.evict_lru();
        }

        // Load the model
        log::info!(
            "[TRANSCRIPTION_DEBUG] Loading new model into cache: {}",
            key
        );
        let start = std::time::Instant::now();

        let transcriber = match Transcriber::new(model_path, speed_mode) {
            Ok(t) => {
                let elapsed = start.elapsed();
                log::info!(
                    "[TRANSCRIPTION_DEBUG] Model loaded successfully in {:?}",
                    elapsed
                );
                Arc::new(t)
            }
            Err(e) => {
                log::error!("[TRANSCRIPTION_DEBUG] Failed to load model: {}", e);
                crate::telemetry::capture_model_load_failure(
                    model_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown"),
                );
                return Err(e);
            }
        };

        // Insert into cache
        self.map.insert(key.clone(), transcriber.clone());
        self.lru_order.push_back(key.clone());
        log::info!(
            "[TRANSCRIPTION_DEBUG] Model cached successfully. Cache size: {}/{}",
            self.map.len(),
            self.max_size
        );

        Ok(transcriber)
    }

    /// Update LRU order when a model is accessed
    fn update_lru(&mut self, key: &str) {
        // Remove from current position
        self.lru_order.retain(|k| k != key);
        // Add to end (most recently used)
        self.lru_order.push_back(key.to_string());
    }

    /// Evict the least recently used model
    fn evict_lru(&mut self) {
        if let Some(key) = self.lru_order.pop_front() {
            log::info!("Evicting model from cache: {}", key);

            // Log model cleanup with context
            log_with_context(
                log::Level::Info,
                "Model cleanup started",
                &[
                    ("operation", "MODEL_CLEANUP"),
                    ("model_path", &key),
                    ("reason", "cache_eviction"),
                    ("cache_size", self.map.len().to_string().as_str()),
                ],
            );

            // Remove from cache - this will drop the Arc<Transcriber>
            if let Some(transcriber) = self.map.remove(&key) {
                let ref_count = Arc::strong_count(&transcriber);
                log_with_context(
                    log::Level::Debug,
                    "Model cleanup complete",
                    &[
                        ("operation", "MODEL_CLEANUP"),
                        ("model_path", &key),
                        ("ref_count_before_drop", ref_count.to_string().as_str()),
                        ("cleanup_result", "success"),
                    ],
                );
            } else {
                log_with_context(
                    log::Level::Warn,
                    "Model cleanup warning",
                    &[
                        ("operation", "MODEL_CLEANUP"),
                        ("model_path", &key),
                        ("issue", "model_not_found_in_cache"),
                    ],
                );
            }
        } else {
            log_with_context(
                log::Level::Debug,
                "Model cleanup skipped",
                &[("operation", "MODEL_CLEANUP"), ("reason", "cache_empty")],
            );
        }
    }

    /// Clear the cache, dropping every cached `Arc<Transcriber>` (and, when that was the
    /// last reference, freeing its Whisper/Metal GPU buffers). Called on process exit to
    /// empty the ggml-metal residency set before teardown so the device destructor does
    /// not assert (#28); also usable to free RAM or after a model upgrade.
    pub fn clear(&mut self) {
        self.map.clear();
        self.lru_order.clear();
    }

    /// Get the current number of cached models
    #[cfg(test)]
    pub fn size(&self) -> usize {
        self.map.len()
    }

    /// Get the maximum cache size
    #[cfg(test)]
    pub fn capacity(&self) -> usize {
        self.max_size
    }
}
