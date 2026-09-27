//! Cohere Transcribe cloud STT via `/v2/audio/transcriptions`.
//!
//! Cohere requires an explicit `language` form field; we default to `en` when
//! the caller does not supply one.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.cohere.com/v1/models",
        AuthScheme::Bearer,
        key,
        "Cohere",
    )
    .await
    .map_err(|e| e.message("Cohere"))
}

pub(super) async fn transcribe_typed(
    _app: &AppHandle,
    key: &str,
    model: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    transcribe_at("https://api.cohere.com", key, model, audio_path, language).await
}

pub(super) async fn transcribe_at(
    base_url: &str,
    key: &str,
    model: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    use reqwest::multipart::{Form, Part};
    use tokio::fs;

    let bytes = fs::read(audio_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;
    let filename = audio_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();

    // Cohere requires a language; default to English when unspecified.
    let lang = language
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .unwrap_or("en")
        .to_string();
    let url = format!("{}/v2/audio/transcriptions", base_url);

    let client = common::http_client();
    common::with_retry(|| {
        let bytes = bytes.clone();
        let client = client.clone();
        let filename = filename.clone();
        let lang = lang.clone();
        let model = model.to_string();
        let url = url.clone();
        async move {
            let file_part = Part::bytes(bytes)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| common::SttError::BadResponse)?;
            let form = Form::new()
                .part("file", file_part)
                .text("model", model)
                .text("language", lang);

            let resp = client
                .post(&url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;

            common::parse_text_response(resp, "Cohere transcription").await
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::{common, transcribe_at};
    use std::io::Write;
    use tempfile::NamedTempFile;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn audio_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"wav").unwrap();
        file
    }

    #[tokio::test]
    async fn transcribe_at_posts_language_and_parses_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/audio/transcriptions"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "ok"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let text = transcribe_at(
            &server.uri(),
            "k",
            "cohere-transcribe-03-2026",
            audio.path(),
            None,
        )
        .await
        .unwrap();

        assert_eq!(text, "ok");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let content_type = requests[0]
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        let body = String::from_utf8_lossy(&requests[0].body);
        assert!(body.contains("name=\"language\""));
        assert!(body.contains("\r\n\r\nen\r\n"));
        assert!(body.contains("name=\"model\""));
        assert!(body.contains("\r\n\r\ncohere-transcribe-03-2026\r\n"));
    }

    #[tokio::test]
    async fn transcribe_at_maps_auth_error_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let error = transcribe_at(
            &server.uri(),
            "k",
            "cohere-transcribe-03-2026",
            audio.path(),
            None,
        )
        .await
        .unwrap_err();

        assert!(matches!(error, common::SttError::Auth));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}
