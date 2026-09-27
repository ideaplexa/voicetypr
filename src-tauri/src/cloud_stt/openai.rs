//! OpenAI cloud STT via the OpenAI-compatible `/v1/audio/transcriptions`.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

const BASE: &str = "https://api.openai.com/v1";

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.openai.com/v1/models",
        AuthScheme::Bearer,
        key,
        "OpenAI",
    )
    .await
    .map_err(|e| e.message("OpenAI"))
}

pub(super) async fn transcribe_typed(
    app: &AppHandle,
    key: &str,
    model: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    // The personal dictionary is reused as the recognizer's initial prompt so
    // jargon/brand names are reconciled against the audio at recognition time.
    let prompt = crate::commands::audio::compile_remote_request_context(app, language);
    common::openai_compatible_transcribe(
        BASE,
        key,
        model,
        audio_path,
        language,
        prompt.as_deref(),
        "OpenAI transcription",
    )
    .await
}
