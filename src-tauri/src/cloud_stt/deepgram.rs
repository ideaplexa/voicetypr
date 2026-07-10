//! Deepgram cloud STT via the pre-recorded `/v1/listen` endpoint.
//!
//! Deepgram differs from the OpenAI-compatible providers: `Token` auth (not
//! Bearer), a raw audio body (not multipart), the model in the query string,
//! and a nested transcript path in the response.

use super::common::{self, AuthScheme};
use std::path::Path;
use tauri::AppHandle;

pub(super) const MODEL: &str = "nova-3";

// /v1/auth/token validates any key; /v1/projects needs `project:read`, absent on default tokens.
pub(super) async fn validate_key(key: &str) -> Result<(), String> {
    common::get_validate(
        "https://api.deepgram.com/v1/auth/token",
        AuthScheme::Token,
        key,
        "Deepgram",
    )
    .await
    .map_err(|e| e.message("Deepgram"))
}
/// Returns true for the Nova-3 model family, the only Deepgram models that
/// accept the repeatable `keyterm` query param. Nova-2 exposes a different
/// `keywords` knob (out of scope), so vocabulary terms are emitted only here.
pub(super) fn is_nova3(model: &str) -> bool {
    model.eq_ignore_ascii_case("nova-3") || model.to_lowercase().starts_with("nova-3-")
}

/// Build the repeatable query params for a `/v1/listen` request. The model and
/// `smart_format=true` are always present; `diarize` and `language` are added
/// when requested. Vocabulary `keyterm` params are appended (one per term, in
/// order) only for Nova-3 model ids.
fn build_listen_params(
    model: &str,
    language: Option<&str>,
    keyterms: &[String],
    diarize: bool,
) -> Vec<(&'static str, String)> {
    let mut params: Vec<(&'static str, String)> = vec![
        ("model", model.to_string()),
        ("smart_format", "true".to_string()),
    ];
    if diarize {
        params.push(("diarize", "true".to_string()));
    }
    if let Some(lang) = language.map(str::trim).filter(|lang| !lang.is_empty()) {
        params.push(("language", lang.to_string()));
    }
    if is_nova3(model) {
        for term in keyterms {
            params.push(("keyterm", term.clone()));
        }
    }
    params
}

/// Load the personal dictionary and compile it into Deepgram keyterms. Failures
/// to read settings are non-fatal (transcription continues without vocabulary),
/// mirroring how Soniox loads its context.
fn compile_keyterms(app: &AppHandle, language: Option<&str>) -> Vec<String> {
    match crate::writing::load_writing_settings(app) {
        Ok(settings) => crate::writing::compile_deepgram_keyterms(&settings, language),
        Err(err) => {
            log::warn!(
                "Failed to load writing settings for Deepgram keyterms; continuing without vocabulary: {err}"
            );
            Vec::new()
        }
    }
}

pub(super) async fn transcribe_typed(
    app: &AppHandle,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<String, common::SttError> {
    let keyterms = compile_keyterms(app, language);
    transcribe_at(
        "https://api.deepgram.com",
        key,
        audio_path,
        language,
        MODEL,
        &keyterms,
    )
    .await
}

pub(super) async fn transcribe_at(
    base_url: &str,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
    model: &str,
    keyterms: &[String],
) -> Result<String, common::SttError> {
    use tokio::fs;

    let bytes = fs::read(audio_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;

    let params = build_listen_params(model, language, keyterms, false);
    let endpoint = format!("{}/v1/listen", base_url);

    let client = common::http_client();
    common::with_retry(|| {
        let body = bytes.clone();
        let client = client.clone();
        let params = params.clone();
        let endpoint = endpoint.clone();
        async move {
            let resp = client
                .post(&endpoint)
                .query(&params)
                .header("Authorization", format!("Token {}", key))
                .header("Content-Type", "audio/wav")
                .body(body)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;

            if !resp.status().is_success() {
                return Err(common::log_http_body(resp, "Deepgram transcription").await);
            }

            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|_| common::SttError::BadResponse)?;
            json.pointer("/results/channels/0/alternatives/0/transcript")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or(common::SttError::BadResponse)
        }
    })
    .await
}

pub(super) async fn transcribe_typed_diarized(
    app: &AppHandle,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
) -> Result<super::CloudTranscript, common::SttError> {
    let keyterms = compile_keyterms(app, language);
    transcribe_at_diarized(
        "https://api.deepgram.com",
        key,
        audio_path,
        language,
        MODEL,
        &keyterms,
    )
    .await
}

pub(super) async fn transcribe_at_diarized(
    base_url: &str,
    key: &str,
    audio_path: &Path,
    language: Option<&str>,
    model: &str,
    keyterms: &[String],
) -> Result<super::CloudTranscript, common::SttError> {
    use tokio::fs;

    let bytes = fs::read(audio_path)
        .await
        .map_err(|_| common::SttError::BadResponse)?;

    let params = build_listen_params(model, language, keyterms, true);
    let endpoint = format!("{}/v1/listen", base_url);

    let client = common::http_client();
    common::with_retry(|| {
        let body = bytes.clone();
        let client = client.clone();
        let params = params.clone();
        let endpoint = endpoint.clone();
        async move {
            let resp = client
                .post(&endpoint)
                .query(&params)
                .header("Authorization", format!("Token {}", key))
                .header("Content-Type", "audio/wav")
                .body(body)
                .send()
                .await
                .map_err(|e| common::classify_reqwest_err(&e))?;

            if !resp.status().is_success() {
                return Err(common::log_http_body(resp, "Deepgram diarized transcription").await);
            }

            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|_| common::SttError::BadResponse)?;
            parse_diarized_response(json).ok_or(common::SttError::BadResponse)
        }
    })
    .await
}

fn parse_diarized_response(json: serde_json::Value) -> Option<super::CloudTranscript> {
    let alt = json.pointer("/results/channels/0/alternatives/0")?;
    let text = alt.get("transcript")?.as_str()?.to_string();
    let words = alt
        .get("words")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_deepgram_word).collect())
        .unwrap_or_default();
    Some(super::CloudTranscript { text, words })
}

fn parse_deepgram_word(w: &serde_json::Value) -> Option<crate::transcription::TranscriptionWord> {
    let text = w
        .get("punctuated_word")
        .and_then(|v| v.as_str())
        .or_else(|| w.get("word").and_then(|v| v.as_str()))?
        .to_string();
    let start_ms = w
        .get("start")
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0).round() as u64);
    let end_ms = w
        .get("end")
        .and_then(|v| v.as_f64())
        .map(|s| (s * 1000.0).round() as u64);
    let speaker_id = w
        .get("speaker")
        .and_then(|v| v.as_i64())
        .map(|n| format!("Speaker {n}"));
    let confidence = w
        .get("confidence")
        .and_then(|v| v.as_f64())
        .map(|c| c as f32);
    Some(crate::transcription::TranscriptionWord {
        text,
        start_ms,
        end_ms,
        speaker_id,
        confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        common, parse_deepgram_word, parse_diarized_response, transcribe_at, transcribe_at_diarized,
    };
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
    async fn transcribe_at_posts_token_auth_raw_body_and_parses_nested_transcript() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/listen"))
            .and(header("authorization", "Token k"))
            .and(header("content-type", "audio/wav"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": {
                    "channels": [{
                        "alternatives": [{
                            "transcript": "hi"
                        }]
                    }]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let text = transcribe_at(
            &server.uri(),
            "k",
            audio.path(),
            Some("en"),
            super::MODEL,
            &[],
        )
        .await
        .unwrap();

        assert_eq!(text, "hi");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].body.is_empty());
        assert_eq!(
            requests[0].url.query(),
            Some("model=nova-3&smart_format=true&language=en")
        );
    }

    #[tokio::test]
    async fn transcribe_at_maps_auth_error_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/listen"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let error = transcribe_at(&server.uri(), "k", audio.path(), None, super::MODEL, &[])
            .await
            .unwrap_err();

        assert!(matches!(error, common::SttError::Auth));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
    #[tokio::test]
    async fn transcribe_at_appends_nova3_keyterm_params() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/listen"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": {
                    "channels": [{ "alternatives": [{ "transcript": "hi" }] }]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();
        let keyterms = vec!["shadcn ui".to_string(), "Tauri".to_string()];

        transcribe_at(
            &server.uri(),
            "k",
            audio.path(),
            Some("en"),
            super::MODEL,
            &keyterms,
        )
        .await
        .unwrap();

        let request = &server.received_requests().await.unwrap()[0];
        let emitted: Vec<String> = request
            .url
            .query_pairs()
            .filter(|(k, _)| k.as_ref() == "keyterm")
            .map(|(_, v)| v.to_string())
            .collect();
        assert_eq!(emitted, vec!["shadcn ui".to_string(), "Tauri".to_string()]);
        // Terms with spaces/slashes are URL-encoded by reqwest but decode back
        // intact, and the model stays Nova-3 with smart_format on.
        let model = request
            .url
            .query_pairs()
            .find(|(k, _)| k.as_ref() == "model")
            .map(|(_, v)| v.to_string());
        assert_eq!(model, Some("nova-3".to_string()));
    }

    #[tokio::test]
    async fn transcribe_at_omits_keyterm_params_for_non_nova3_model() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/listen"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": {
                    "channels": [{ "alternatives": [{ "transcript": "hi" }] }]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();
        // Terms are supplied but Nova-2 has no keyterm knob, so none may be sent.
        let keyterms = vec!["shadcn ui".to_string()];

        transcribe_at(
            &server.uri(),
            "k",
            audio.path(),
            Some("en"),
            "nova-2",
            &keyterms,
        )
        .await
        .unwrap();

        let request = &server.received_requests().await.unwrap()[0];
        let has_keyterm = request
            .url
            .query_pairs()
            .any(|(k, _)| k.as_ref() == "keyterm");
        assert!(!has_keyterm);
        let model = request
            .url
            .query_pairs()
            .find(|(k, _)| k.as_ref() == "model")
            .map(|(_, v)| v.to_string());
        assert_eq!(model, Some("nova-2".to_string()));
    }

    #[tokio::test]
    async fn transcribe_at_diarized_appends_nova3_keyterm_params() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/listen"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "results": {
                    "channels": [{
                        "alternatives": [{ "transcript": "hi", "words": [] }]
                    }]
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();
        let keyterms = vec!["Tauri".to_string()];

        transcribe_at_diarized(
            &server.uri(),
            "k",
            audio.path(),
            Some("en"),
            super::MODEL,
            &keyterms,
        )
        .await
        .unwrap();

        let request = &server.received_requests().await.unwrap()[0];
        let emitted: Vec<String> = request
            .url
            .query_pairs()
            .filter(|(k, _)| k.as_ref() == "keyterm")
            .map(|(_, v)| v.to_string())
            .collect();
        assert_eq!(emitted, vec!["Tauri".to_string()]);
        let diarize = request
            .url
            .query_pairs()
            .find(|(k, _)| k.as_ref() == "diarize")
            .map(|(_, v)| v.to_string());
        assert_eq!(diarize, Some("true".to_string()));
    }

    #[test]
    fn parse_deepgram_word_uses_punctuated_word_and_maps_fields() {
        let w = serde_json::json!({
            "word": "hello",
            "punctuated_word": "Hello,",
            "start": 0.5,
            "end": 1.25,
            "confidence": 0.98,
            "speaker": 1
        });
        let word = parse_deepgram_word(&w).unwrap();
        assert_eq!(word.text, "Hello,");
        assert_eq!(word.start_ms, Some(500));
        assert_eq!(word.end_ms, Some(1250));
        assert_eq!(word.speaker_id, Some("Speaker 1".to_string()));
        assert!((word.confidence.unwrap() - 0.98_f32).abs() < 0.001);
    }

    #[test]
    fn parse_deepgram_word_falls_back_to_word_when_no_punctuated() {
        let w = serde_json::json!({
            "word": "world",
            "start": 1.0,
            "end": 1.5,
            "confidence": 0.9,
            "speaker": 0
        });
        let word = parse_deepgram_word(&w).unwrap();
        assert_eq!(word.text, "world");
        assert_eq!(word.speaker_id, Some("Speaker 0".to_string()));
    }

    #[test]
    fn parse_deepgram_word_returns_none_for_missing_text() {
        let w = serde_json::json!({ "start": 0.0, "end": 0.5 });
        assert!(parse_deepgram_word(&w).is_none());
    }

    #[test]
    fn parse_diarized_response_two_speakers_multiple_words() {
        let json = serde_json::json!({
            "results": {
                "channels": [{
                    "alternatives": [{
                        "transcript": "Hello world thanks",
                        "words": [
                            {
                                "word": "hello", "punctuated_word": "Hello",
                                "start": 0.0, "end": 0.5,
                                "confidence": 0.99, "speaker": 0
                            },
                            {
                                "word": "world", "punctuated_word": "world",
                                "start": 0.6, "end": 1.1,
                                "confidence": 0.95, "speaker": 0
                            },
                            {
                                "word": "thanks", "punctuated_word": "thanks.",
                                "start": 1.5, "end": 2.0,
                                "confidence": 0.88, "speaker": 1
                            }
                        ]
                    }]
                }]
            }
        });
        let ct = parse_diarized_response(json).unwrap();
        assert_eq!(ct.text, "Hello world thanks");
        assert_eq!(ct.words.len(), 3);
        assert_eq!(ct.words[0].text, "Hello");
        assert_eq!(ct.words[0].speaker_id, Some("Speaker 0".to_string()));
        assert_eq!(ct.words[0].start_ms, Some(0));
        assert_eq!(ct.words[0].end_ms, Some(500));
        assert_eq!(ct.words[2].text, "thanks.");
        assert_eq!(ct.words[2].speaker_id, Some("Speaker 1".to_string()));
        assert_eq!(ct.words[2].start_ms, Some(1500));
        assert_eq!(ct.words[2].end_ms, Some(2000));
    }

    #[test]
    fn parse_diarized_response_no_words_field_gives_empty_vec() {
        let json = serde_json::json!({
            "results": {
                "channels": [{
                    "alternatives": [{
                        "transcript": "hi there"
                    }]
                }]
            }
        });
        let ct = parse_diarized_response(json).unwrap();
        assert_eq!(ct.text, "hi there");
        assert!(ct.words.is_empty());
    }
}
