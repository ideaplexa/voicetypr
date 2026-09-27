//! Real transcription context for the remote HTTP server
//!
//! Implements ServerContext using actual Whisper or Parakeet transcription.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::Instant;
use tauri::AppHandle;
use tempfile::NamedTempFile;

use super::http::ServerContext;
use super::server::RemoteModelControlSnapshot;
use crate::parakeet::messages::{ParakeetResponse, ParakeetSegment};
use crate::parakeet::ParakeetManager;
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSegment, TranscriptionSource,
};
use crate::whisper::cache::TranscriberCache;

/// Configuration for the transcription server
#[derive(Clone)]
pub struct TranscriptionServerConfig {
    /// Server's display name (e.g., "Desktop-PC")
    pub server_name: String,
    /// Password for authentication (None = no auth required)
    pub password: Option<String>,
    /// Path to the currently selected model
    pub model_path: PathBuf,
    /// Name of the current model (e.g., "large-v3-turbo")
    pub model_name: String,
}

/// Current model configuration served by the remote server.
#[derive(Clone)]
pub struct SharedModelState {
    /// Current model name.
    pub model_name: String,
    /// Current model path, only used by Whisper.
    pub model_path: PathBuf,
    /// Current engine type (whisper, parakeet, etc.).
    pub engine: String,
}

/// Shared state that can be updated while the server is running
#[derive(Clone)]
pub struct SharedServerState {
    state: Arc<RwLock<SharedModelState>>,
}

impl SharedServerState {
    /// Create new shared state from initial values
    pub fn new(model_name: String, model_path: PathBuf, engine: String) -> Self {
        Self {
            state: Arc::new(RwLock::new(SharedModelState {
                model_name,
                model_path,
                engine,
            })),
        }
    }

    /// Update the model atomically.
    pub fn update_model(&self, model_name: String, model_path: PathBuf, engine: String) {
        if let Ok(mut state) = self.state.write() {
            *state = SharedModelState {
                model_name,
                model_path,
                engine,
            };
        }
    }

    /// Snapshot the current model state atomically.
    pub fn snapshot(&self) -> SharedModelState {
        self.state
            .read()
            .map(|state| state.clone())
            .unwrap_or_else(|_| SharedModelState {
                model_name: String::new(),
                model_path: PathBuf::new(),
                engine: "whisper".to_string(),
            })
    }

    /// Get the current model name
    pub fn get_model_name(&self) -> String {
        self.snapshot().model_name
    }

    /// Get the current model path (test-only accessor)
    #[cfg(test)]
    pub fn get_model_path(&self) -> PathBuf {
        self.snapshot().model_path
    }

    /// Get the current engine
    pub fn get_engine(&self) -> String {
        self.snapshot().engine
    }
}

/// Real transcription context that uses Whisper or Parakeet
///
/// Uses std::sync::Mutex for the cache because transcription is inherently
/// blocking/CPU-bound and we want to serialize requests (as per design).
pub struct RealTranscriptionContext {
    /// Server name (static)
    server_name: String,
    /// Password for authentication
    password: Option<String>,
    /// Shared state for dynamic model updates
    shared_state: SharedServerState,
    /// Cache for loaded transcriber models - uses std Mutex for blocking access
    cache: Arc<StdMutex<TranscriberCache>>,
    /// AppHandle for accessing Parakeet manager (optional, needed for parakeet engine)
    app_handle: Option<AppHandle>,
}

impl RealTranscriptionContext {
    /// Create a new transcription context with shared state and AppHandle
    pub fn new_with_shared_state(
        server_name: String,
        password: Option<String>,
        shared_state: SharedServerState,
        app_handle: Option<AppHandle>,
    ) -> Self {
        Self {
            server_name,
            password,
            shared_state,
            cache: Arc::new(StdMutex::new(TranscriberCache::new())),
            app_handle,
        }
    }

    /// Create a new transcription context (legacy, creates its own shared state)
    #[cfg(test)]
    pub fn new(config: TranscriptionServerConfig) -> Self {
        // Default to whisper engine for legacy compatibility
        let shared_state =
            SharedServerState::new(config.model_name, config.model_path, "whisper".to_string());
        Self {
            server_name: config.server_name,
            password: config.password,
            shared_state,
            cache: Arc::new(StdMutex::new(TranscriberCache::new())),
            app_handle: None,
        }
    }

    /// Update the model being served
    #[cfg(test)]
    pub fn update_model(&mut self, model_path: PathBuf, model_name: String, engine: String) {
        self.shared_state
            .update_model(model_name, model_path, engine);
    }

    /// Update the password
    #[cfg(test)]
    pub fn update_password(&mut self, password: Option<String>) {
        self.password = password;
    }

    /// Get the current model path
    #[cfg(test)]
    pub fn get_model_path(&self) -> PathBuf {
        self.shared_state.get_model_path()
    }

    async fn model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
        let app = self
            .app_handle
            .as_ref()
            .ok_or_else(|| "Remote control requires app context".to_string())?;

        crate::remote::model_control::build_remote_model_control_snapshot(app, &self.shared_state)
            .await
    }

    async fn set_shared_model(
        &self,
        model_id: String,
        engine: String,
    ) -> Result<RemoteModelControlSnapshot, String> {
        let app = self
            .app_handle
            .as_ref()
            .ok_or_else(|| "Remote control requires app context".to_string())?;

        crate::remote::model_control::update_remote_shared_model(
            app,
            &self.shared_state,
            &model_id,
            &engine,
        )
        .await
    }
}

fn parakeet_segments_to_transcription_segments(
    segments: Vec<ParakeetSegment>,
) -> Vec<TranscriptionSegment> {
    segments
        .into_iter()
        .map(|segment| TranscriptionSegment {
            text: segment.text,
            start_ms: segment.start.map(|value| (value.max(0.0) * 1000.0) as u64),
            end_ms: segment.end.map(|value| (value.max(0.0) * 1000.0) as u64),
            speaker_id: None,
        })
        .collect()
}

impl ServerContext for RealTranscriptionContext {
    fn get_model_name(&self) -> String {
        // Read current model name from shared state (dynamic)
        self.shared_state.get_model_name()
    }

    fn get_server_name(&self) -> String {
        self.server_name.clone()
    }

    fn get_password(&self) -> Option<String> {
        self.password.clone()
    }

    fn get_engine(&self) -> String {
        self.shared_state.get_engine()
    }

    fn model_status_snapshot(&self) -> (String, String) {
        let snap = self.shared_state.snapshot();
        (snap.engine, snap.model_name)
    }

    fn transcribe(
        &self,
        audio_data: &[u8],
        spoken_language: Option<&str>,
        transcription_task: Option<&str>,
    ) -> Result<TranscriptionResult, String> {
        self.transcribe_inner(audio_data, spoken_language, transcription_task, None)
    }

    fn transcribe_with_context(
        &self,
        audio_data: &[u8],
        spoken_language: Option<&str>,
        transcription_task: Option<&str>,
        context: Option<&str>,
    ) -> Result<TranscriptionResult, String> {
        self.transcribe_inner(audio_data, spoken_language, transcription_task, context)
    }

    fn get_model_control_snapshot(&self) -> Result<RemoteModelControlSnapshot, String> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.model_control_snapshot())
        })
    }

    fn update_shared_model(
        &self,
        model_id: &str,
        engine: &str,
    ) -> Result<RemoteModelControlSnapshot, String> {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.set_shared_model(model_id.to_string(), engine.to_string()))
        })
    }
}

impl RealTranscriptionContext {
    fn transcribe_inner(
        &self,
        audio_data: &[u8],
        spoken_language: Option<&str>,
        transcription_task: Option<&str>,
        context: Option<&str>,
    ) -> Result<TranscriptionResult, String> {
        let start = Instant::now();

        // Get current engine and model info from one atomic shared-state snapshot.
        let SharedModelState {
            model_name,
            model_path,
            engine,
        } = self.shared_state.snapshot();
        let translate_to_english = transcription_task
            == Some(crate::commands::settings::TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH);
        let job = TranscriptionJob::from_legacy_settings(
            TranscriptionSource::RemoteServer,
            engine.clone(),
            model_name.clone(),
            spoken_language.map(str::to_string),
            translate_to_english,
        );

        log::info!(
            "Starting remote transcription: {} bytes of audio, engine='{}', model='{}', context_bytes={}",
            audio_data.len(),
            engine,
            model_name,
            context.map(str::len).unwrap_or(0)
        );

        // Validate audio data is not empty
        if audio_data.is_empty() {
            return Err("Empty audio data".to_string());
        }

        // Write audio data to a temporary WAV file
        let mut temp_file =
            NamedTempFile::new().map_err(|e| format!("Failed to create temp file: {}", e))?;

        temp_file
            .write_all(audio_data)
            .map_err(|e| format!("Failed to write audio data: {}", e))?;

        temp_file
            .flush()
            .map_err(|e| format!("Failed to flush temp file: {}", e))?;

        let temp_path = temp_file.path().to_path_buf();

        log::info!("Audio written to temp file: {:?}", temp_path);

        // Route to appropriate transcription engine
        let result = match engine.as_str() {
            "parakeet" => self.transcribe_with_parakeet(
                &temp_path,
                &model_name,
                &job,
                spoken_language.map(str::to_string),
                translate_to_english,
            )?,
            _ => {
                let output = self.transcribe_with_whisper(
                    &temp_path,
                    &model_path,
                    spoken_language,
                    translate_to_english,
                    context,
                )?;
                TranscriptionResult::new(&job, output.raw_text)
                    .with_transcript_language(output.transcript_language)
                    .with_segments(output.segments)
                    .with_audio_duration_ms(Some(output.audio_duration_ms))
            }
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        let result = result.with_processing_duration_ms(Some(duration_ms));

        log::info!(
            "Remote transcription completed: {} chars in {}ms using {} ({})",
            result.raw_text.len(),
            duration_ms,
            model_name,
            engine
        );

        Ok(result)
    }

    /// Transcribe using Whisper model
    fn transcribe_with_whisper(
        &self,
        audio_path: &Path,
        model_path: &Path,
        spoken_language: Option<&str>,
        translate_to_english: bool,
        context: Option<&str>,
    ) -> Result<crate::whisper::transcriber::WhisperTranscriptionOutput, String> {
        // Get transcriber from cache (blocking lock - serializes all transcriptions)
        let transcriber = {
            let mut cache = self
                .cache
                .lock()
                .map_err(|e| format!("Failed to acquire cache lock: {}", e))?;
            let speed_mode = self
                .app_handle
                .as_ref()
                .map(crate::commands::settings::read_whisper_speed_mode)
                .unwrap_or(false);

            cache
                .get_or_create(model_path, speed_mode)
                .map_err(|e| format!("Failed to load Whisper model: {}", e))?
        };

        log::info!("Whisper model loaded, starting transcription...");

        // Perform transcription (this can take a while)
        transcriber
            .transcribe_with_metadata_with_prompt(
                audio_path,
                spoken_language,
                translate_to_english,
                context,
                None,
                || false,
            )
            .map_err(|e| format!("Whisper transcription failed: {}", e))
    }

    /// Transcribe using Parakeet sidecar
    fn transcribe_with_parakeet(
        &self,
        audio_path: &Path,
        model_name: &str,
        job: &TranscriptionJob,
        spoken_language: Option<String>,
        translate_to_english: bool,
    ) -> Result<TranscriptionResult, String> {
        let app_handle = self.app_handle.as_ref().ok_or_else(|| {
            "Parakeet transcription requires AppHandle but none was provided".to_string()
        })?;

        log::info!(
            "Using Parakeet engine for transcription with model '{}'",
            model_name
        );

        // Get the ParakeetManager from app state
        use tauri::Manager;
        let parakeet_manager = app_handle.state::<ParakeetManager>();

        // Clone what we need for the async block
        let app_handle_clone = app_handle.clone();
        let model_name_owned = model_name.to_string();
        let audio_path_clone = audio_path.to_path_buf();

        // Run the async Parakeet transcription in a blocking context
        // Since we're already in a sync context (ServerContext::transcribe), use block_on
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // First, ensure the model is loaded
                parakeet_manager
                    .load_model(&app_handle_clone, &model_name_owned)
                    .await
                    .map_err(|e| format!("Failed to load Parakeet model: {}", e))?;

                // Perform transcription
                let response = parakeet_manager
                    .transcribe(
                        &app_handle_clone,
                        &model_name_owned,
                        audio_path_clone,
                        spoken_language.clone(),
                        translate_to_english,
                        None,
                    )
                    .await
                    .map_err(|e| format!("Parakeet transcription failed: {}", e))?;

                match response {
                    ParakeetResponse::Transcription {
                        text,
                        segments,
                        language,
                        duration,
                    } => Ok::<TranscriptionResult, String>(
                        TranscriptionResult::new(job, text)
                            .with_transcript_language(language)
                            .with_segments(parakeet_segments_to_transcription_segments(segments))
                            .with_audio_duration_ms(
                                duration.map(|value| (value.max(0.0) * 1000.0) as u64),
                            ),
                    ),
                    ParakeetResponse::Error { code, message, .. } => {
                        Err(format!("Parakeet error {}: {}", code, message))
                    }
                    other => Err(format!("Unexpected Parakeet response: {:?}", other)),
                }
            })
        });

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcription_server_config() {
        let config = TranscriptionServerConfig {
            server_name: "Test Server".to_string(),
            password: Some("secret".to_string()),
            model_path: PathBuf::from("/models/test.bin"),
            model_name: "test-model".to_string(),
        };

        assert_eq!(config.server_name, "Test Server");
        assert_eq!(config.password, Some("secret".to_string()));
        assert_eq!(config.model_name, "test-model");
    }

    #[test]
    fn test_real_context_creation() {
        let config = TranscriptionServerConfig {
            server_name: "Desktop-PC".to_string(),
            password: None,
            model_path: PathBuf::from("/models/large-v3-turbo.bin"),
            model_name: "large-v3-turbo".to_string(),
        };

        let ctx = RealTranscriptionContext::new(config);

        assert_eq!(ctx.get_model_name(), "large-v3-turbo");
        assert_eq!(ctx.get_server_name(), "Desktop-PC");
        assert!(ctx.get_password().is_none());
    }

    #[test]
    fn test_context_with_password() {
        let config = TranscriptionServerConfig {
            server_name: "Secure Server".to_string(),
            password: Some("mypassword".to_string()),
            model_path: PathBuf::from("/models/base.en.bin"),
            model_name: "base.en".to_string(),
        };

        let ctx = RealTranscriptionContext::new(config);

        assert_eq!(ctx.get_password(), Some("mypassword".to_string()));
    }

    #[test]
    fn test_context_update_model() {
        let config = TranscriptionServerConfig {
            server_name: "Test".to_string(),
            password: None,
            model_path: PathBuf::from("/models/old.bin"),
            model_name: "old-model".to_string(),
        };

        let mut ctx = RealTranscriptionContext::new(config);

        ctx.update_model(
            PathBuf::from("/models/new.bin"),
            "new-model".to_string(),
            "whisper".to_string(),
        );

        assert_eq!(ctx.get_model_name(), "new-model");
        assert_eq!(ctx.get_model_path(), PathBuf::from("/models/new.bin"));
    }

    #[test]
    fn test_context_update_password() {
        let config = TranscriptionServerConfig {
            server_name: "Test".to_string(),
            password: None,
            model_path: PathBuf::from("/models/test.bin"),
            model_name: "test".to_string(),
        };

        let mut ctx = RealTranscriptionContext::new(config);

        assert!(ctx.get_password().is_none());

        ctx.update_password(Some("newsecret".to_string()));
        assert_eq!(ctx.get_password(), Some("newsecret".to_string()));

        ctx.update_password(None);
        assert!(ctx.get_password().is_none());
    }

    #[test]
    fn test_transcribe_empty_audio_returns_error() {
        let config = TranscriptionServerConfig {
            server_name: "Test".to_string(),
            password: None,
            model_path: PathBuf::from("/models/test.bin"),
            model_name: "test".to_string(),
        };

        let ctx = RealTranscriptionContext::new(config);

        // Empty audio should return error
        let result = ctx.transcribe(&[], None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Empty audio data"));
    }

    #[test]
    fn test_transcribe_invalid_model_path_returns_error() {
        let config = TranscriptionServerConfig {
            server_name: "Test".to_string(),
            password: None,
            model_path: PathBuf::from("/nonexistent/model.bin"),
            model_name: "test".to_string(),
        };

        let ctx = RealTranscriptionContext::new(config);

        // Some fake audio data (not valid WAV, but tests path validation)
        let result = ctx.transcribe(&[1, 2, 3, 4, 5], None, None);
        assert!(result.is_err());
        // Should fail because model doesn't exist
        let error = result.unwrap_err();
        assert!(
            error.contains("Failed to load") || error.contains("No such file"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn model_status_snapshot_matches_atomic_shared_state_read() {
        let shared = SharedServerState::new(
            "nano".to_string(),
            PathBuf::from("/models/nano.bin"),
            "parakeet".to_string(),
        );
        let ctx =
            RealTranscriptionContext::new_with_shared_state("host".to_string(), None, shared, None);
        let (engine, model) = ctx.model_status_snapshot();
        assert_eq!(engine, "parakeet");
        assert_eq!(model, "nano");
    }
}
