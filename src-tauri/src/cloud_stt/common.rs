//! Shared HTTP helpers for cloud STT providers.

use std::path::Path;

/// Authorization header scheme for a provider's REST API.
#[derive(Clone, Copy)]
pub(super) enum AuthScheme {
    /// `Authorization: Bearer <key>`
    Bearer,
    /// `Authorization: Token <key>` (Deepgram)
    Token,
}

#[derive(Debug)]
pub(crate) enum SttError {
    Auth,
    ModelUnavailable,
    RateLimited,
    /// Provider-side quota wall (e.g. Soniox `limit_exceeded`): permanent
    /// until whatever it caps is relieved — never a retry candidate.
    /// `file_storage`: `true` for retained-audio caps (stored file
    /// count/size — only file deletions free them), `false` for
    /// stored-transcription count caps (record deletions free them). The
    /// storage-wall self-heal uses this to wait on capacity that actually
    /// freed before retrying.
    LimitExceeded {
        file_storage: bool,
    },
    Timeout,
    Network,
    Server,
    BadResponse,
}

impl SttError {
    pub(crate) fn message(&self, provider_name: &str) -> String {
        match self {
            Self::Auth => format!("Invalid {} API key", provider_name),
            Self::ModelUnavailable => format!(
                "{}: model unavailable for this API key — check the key's scopes and your plan/credits",
                provider_name
            ),
            Self::RateLimited => {
                format!("{} rate limit reached. Try again shortly.", provider_name)
            }
            Self::LimitExceeded { .. } => format!(
                "{provider} storage quota exceeded. Open Sources → Cloud → Clean up stored files to retry cleanup of records from this app session. Manage older or other-client records in the Soniox console.",
                provider = provider_name
            ),
            Self::Timeout => format!("{} request timed out", provider_name),
            Self::Network => format!("Network error reaching {}", provider_name),
            Self::Server => format!("{} service error. Try again shortly.", provider_name),
            Self::BadResponse => format!("{}: unexpected response", provider_name),
        }
    }
}

pub(super) fn classify_status(status: reqwest::StatusCode) -> SttError {
    match status {
        reqwest::StatusCode::UNAUTHORIZED => SttError::Auth,
        // 403 = valid key lacking scope or model access, not a bad key.
        reqwest::StatusCode::FORBIDDEN => SttError::ModelUnavailable,
        reqwest::StatusCode::NOT_FOUND => SttError::ModelUnavailable,
        reqwest::StatusCode::REQUEST_TIMEOUT => SttError::Timeout,
        reqwest::StatusCode::TOO_MANY_REQUESTS => SttError::RateLimited,
        s if s.is_server_error() => SttError::Server,
        _ => SttError::BadResponse,
    }
}

pub(super) fn classify_reqwest_err(e: &reqwest::Error) -> SttError {
    if e.is_timeout() {
        SttError::Timeout
    } else {
        SttError::Network
    }
}

pub(crate) fn is_transient(err: &SttError) -> bool {
    matches!(
        err,
        SttError::RateLimited | SttError::Timeout | SttError::Network | SttError::Server
    )
}

/// Total deadline for a single cloud STT **transcription/upload/poll** request.
///
/// This MUST be at least as large as the largest deadline a caller hands a
/// cloud request, so that the caller's shared deadline — never reqwest — is
/// what bounds a legitimately long upload. The transcription executor wraps
/// cloud calls in `tokio::time::timeout` using a watchdog budget clamped to
/// 30 min (`transcription_watchdog_budget`, MAX_SECONDS = 30 * 60). The
/// previous hard-coded 120 s cap fired *before* that budget for every request
/// (the floor is 180 s), dropping valid long-file jobs as `SttError::Timeout`.
/// Matching the 30 min ceiling makes the executor/poll budget the binding
/// deadline while still bounding a request that connects but never responds
/// for the direct (non-executor) upload callers and each individual Soniox
/// poll request.
///
/// Only transcription traffic uses this; validation uses the much shorter
/// [`VALIDATE_TIMEOUT`] — see [`build_client`].
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// Short, interactive deadline for an API-key **validation** GET.
///
/// Validation hits a tiny authenticated listing endpoint that answers in well
/// under a second. A connected-but-unresponsive endpoint must fail fast rather
/// than pin the validation command for the transcription budget: `get_validate`
/// retries a transient timeout once via `with_retry`, so the shared 30-min
/// client would have held a hung endpoint for ~1 hour. This stays far below
/// [`REQUEST_TIMEOUT`] so that risk cannot occur on the validation path.
const VALIDATE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Build an HTTPS-only reqwest client with the given per-request deadline.
fn build_client(timeout: std::time::Duration) -> reqwest::Client {
    let builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(std::time::Duration::from_secs(15));
    // Cloud STT sends an API key (Authorization header) and the raw audio in
    // the request body, so the client must never speak plaintext HTTP. Force
    // HTTPS so a misconfigured — or future user-supplied — base URL cannot
    // leak them. reqwest's https_only has no loopback carve-out, and loopback
    // HTTP is used only by the in-tree wiremock tests, so the guard is compiled
    // out under cfg(test) to keep those tests green.
    #[cfg(not(test))]
    let builder = builder.https_only(true);
    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}

#[cfg(not(test))]
static SHARED_CLIENT: std::sync::LazyLock<reqwest::Client> =
    std::sync::LazyLock::new(|| build_client(REQUEST_TIMEOUT));

pub(super) fn http_client() -> reqwest::Client {
    #[cfg(test)]
    {
        // Wiremock servers use ephemeral ports that can be recycled between
        // parallel tests. Isolate their connection pools so a stale keep-alive
        // socket cannot reach a later server that inherited the same port.
        build_client(REQUEST_TIMEOUT)
    }
    #[cfg(not(test))]
    {
        SHARED_CLIENT.clone()
    }
}

// Fire-and-forget: warm the shared pool's DNS+TCP+TLS for `origin`; result ignored.
pub(super) async fn warm_origin(origin: &str) {
    let _ = http_client()
        .head(origin)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await;
}

/// The per-request deadline applied to **validation** traffic. Production uses
/// [`VALIDATE_TIMEOUT`]; tests may shrink it via
/// [`set_validate_timeout_override`] so the behavioral timeout regression test
/// runs in milliseconds instead of waiting on the full 30 s.
fn validate_request_timeout() -> std::time::Duration {
    #[cfg(test)]
    {
        if let Some(ms) = VALIDATE_TIMEOUT_OVERRIDE.with(|c| c.get()) {
            return std::time::Duration::from_millis(ms);
        }
    }
    VALIDATE_TIMEOUT
}

// Test seam: shrink the validation request deadline (millisecond precision).
#[cfg(test)]
thread_local! {
    static VALIDATE_TIMEOUT_OVERRIDE: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn set_validate_timeout_override(ms: Option<u64>) {
    VALIDATE_TIMEOUT_OVERRIDE.with(|c| c.set(ms));
}

pub(super) async fn with_retry<T, F, Fut>(mut op: F) -> Result<T, SttError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, SttError>>,
{
    match op().await {
        Ok(v) => Ok(v),
        Err(e) if is_transient(&e) => {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            op().await
        }
        Err(e) => Err(e),
    }
}

/// Classify a Soniox `limit_exceeded` 429 body by which documented cap it
/// names: `Some(true)` = retained-audio storage (stored file count/size —
/// freed only by file deletions), `Some(false)` = stored-transcription
/// count (freed by record deletions). Soniox folds per-minute request
/// rate, concurrency, pending-file and storage walls into this single
/// `error_type`; per the API docs the `message` names which limit was hit,
/// so the classification anchors on the documented limit identifiers
/// (`files_total_count`, `files_total_size_gb`,
/// `transcribe_async_total_num_files`) and their plain-English example
/// wording. `total` anchors the storage phrases so the transient "pending
/// file count" wall can never match. Any other wording — per-minute rate,
/// concurrency, pending, or future unknown limits — is `None`: transient
/// (`RateLimited`), never a destructive cleanup.
fn soniox_limit_is_file_storage(body: &str) -> Option<bool> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return None;
    };
    if v.get("error_type").and_then(|t| t.as_str()) != Some("limit_exceeded") {
        return None;
    }
    let message = v.get("message").and_then(|m| m.as_str())?;
    let message = message.to_ascii_lowercase();
    if message.contains("transcribe_async_total_num_files") {
        return Some(false);
    }
    if message.contains("files_total_count")
        || message.contains("files_total_size_gb")
        || message.contains("total file count")
        || message.contains("total file size")
    {
        return Some(true);
    }
    None
}

pub(super) async fn log_http_body(resp: reqwest::Response, label: &str) -> SttError {
    log_http_body_for_provider(resp, label, false).await
}

pub(super) async fn log_soniox_http_body(resp: reqwest::Response, label: &str) -> SttError {
    log_http_body_for_provider(resp, label, true).await
}

async fn log_http_body_for_provider(
    resp: reqwest::Response,
    label: &str,
    soniox: bool,
) -> SttError {
    let status = resp.status();
    let mut err = classify_status(status);
    // Parse only to classify quotas; provider bodies can echo arbitrary content.
    let body = resp.text().await.unwrap_or_default();
    // Soniox folds many distinct limits into one HTTP 429
    // `error_type: "limit_exceeded"` (per-minute request rate, concurrency,
    // stored-file count/size, stored-transcription count, pending files).
    // The docs instruct clients to read `message` for the sub-cause: only
    // retained-storage walls are permanent until records are deleted, so
    // only those may become the non-retryable quota error (plan 060) —
    // typed by which capacity was hit so the self-heal waits on capacity
    // that actually freed. Every other 429 — including unrecognized
    // wording — stays the transient `RateLimited`; we never guess a
    // destructive cleanup from an unrecognized message and never surface
    // the raw body to the user.
    if soniox && matches!(err, SttError::RateLimited) {
        if let Some(file_storage) = soniox_limit_is_file_storage(&body) {
            err = SttError::LimitExceeded { file_storage };
        }
    }
    log::warn!("{}", http_failure_log(label, status.as_u16(), &err));
    err
}

fn http_failure_log(label: &str, status: u16, error: &SttError) -> String {
    let provider = ["Soniox", "Deepgram", "OpenAI", "Groq", "Cohere"]
        .into_iter()
        .find(|provider| label.starts_with(*provider))
        .unwrap_or("unknown");
    let code = match error {
        SttError::Auth => "unauthorized",
        SttError::ModelUnavailable => "model_unavailable",
        SttError::RateLimited => "rate_limited",
        SttError::LimitExceeded { .. } => "cloud_storage_limit",
        SttError::Timeout => "timeout",
        SttError::Network => "network",
        SttError::Server => "unavailable",
        SttError::BadResponse => "invalid_response",
    };
    format!("provider={provider} status={status} code={code}")
}

/// Parse a JSON response whose transcript lives at the top-level `text` field
/// (OpenAI / Groq / Cohere shape).
pub(super) async fn parse_text_response(
    resp: reqwest::Response,
    label: &str,
) -> Result<String, SttError> {
    if !resp.status().is_success() {
        return Err(log_http_body(resp, label).await);
    }
    let json: serde_json::Value = resp.json().await.map_err(|_| SttError::BadResponse)?;
    json.get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or(SttError::BadResponse)
}

/// Validate a key with a GET request to an authenticated listing endpoint.
pub(super) async fn get_validate(
    url: &str,
    scheme: AuthScheme,
    key: &str,
    provider_name: &str,
) -> Result<(), SttError> {
    let client = build_client(validate_request_timeout());
    with_retry(|| {
        let client = client.clone();
        async move {
            let req = match scheme {
                AuthScheme::Bearer => client.get(url).bearer_auth(key),
                AuthScheme::Token => client
                    .get(url)
                    .header("Authorization", format!("Token {}", key)),
            };
            let resp = req.send().await.map_err(|e| classify_reqwest_err(&e))?;
            if resp.status().is_success() {
                Ok(())
            } else {
                Err(log_http_body(resp, provider_name).await)
            }
        }
    })
    .await
}

/// OpenAI/Groq only honor the final ~224 tokens of `prompt`, so an oversized
/// dictionary would be truncated from the front by the provider. Cap it here on
/// a separator (~3 chars/token for jargon-dense terms) so whole early terms win.
fn cap_transcription_prompt(prompt: &str) -> String {
    const MAX_BYTES: usize = 672;
    if prompt.len() <= MAX_BYTES {
        return prompt.to_string();
    }
    let mut end = MAX_BYTES;
    while end > 0 && !prompt.is_char_boundary(end) {
        end -= 1;
    }
    let cut = prompt[..end].rfind([',', ' ']).unwrap_or(end);
    prompt[..cut].trim_end_matches([',', ' ']).to_string()
}

/// Transcribe via an OpenAI-compatible multipart `/audio/transcriptions`
/// endpoint (OpenAI, Groq). `base_url` excludes the trailing path.
pub(super) async fn openai_compatible_transcribe(
    base_url: &str,
    key: &str,
    model: &str,
    audio_path: &Path,
    language: Option<&str>,
    prompt: Option<&str>,
    label: &str,
) -> Result<String, SttError> {
    use futures_util::stream;
    use reqwest::{
        multipart::{Form, Part},
        Body,
    };
    use tokio::fs::{metadata, File};
    use tokio::io::AsyncReadExt;

    let audio_len = metadata(audio_path)
        .await
        .map_err(|_| SttError::BadResponse)?
        .len();
    let filename = audio_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();
    let language = language
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .map(str::to_string);
    let prompt = prompt
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .map(cap_transcription_prompt);
    let client = http_client();
    let url = format!("{}/audio/transcriptions", base_url);

    with_retry(|| {
        let client = client.clone();
        let filename = filename.clone();
        let language = language.clone();
        let prompt = prompt.clone();
        let url = url.clone();
        async move {
            let file = File::open(audio_path)
                .await
                .map_err(|_| SttError::BadResponse)?;
            let stream = stream::try_unfold(file, |mut file| async move {
                let mut chunk = vec![0; 64 * 1024];
                let bytes_read = file.read(&mut chunk).await?;
                if bytes_read == 0 {
                    Ok::<Option<(Vec<u8>, File)>, std::io::Error>(None)
                } else {
                    chunk.truncate(bytes_read);
                    Ok(Some((chunk, file)))
                }
            });
            let file_part = Part::stream_with_length(Body::wrap_stream(stream), audio_len)
                .file_name(filename)
                .mime_str("audio/wav")
                .map_err(|_| SttError::BadResponse)?;
            let mut form = Form::new()
                .part("file", file_part)
                .text("model", model.to_string())
                .text("response_format", "json".to_string());
            if let Some(lang) = language {
                // `gpt-transcribe` takes the language as an array field; every
                // other OpenAI-compatible model keeps the singular `language`.
                if model == "gpt-transcribe" {
                    form = form.text("languages[]", lang);
                } else {
                    form = form.text("language", lang);
                }
            }
            if let Some(prompt) = prompt {
                form = form.text("prompt", prompt);
            }

            let resp = client
                .post(&url)
                .bearer_auth(key)
                .multipart(form)
                .send()
                .await
                .map_err(|e| classify_reqwest_err(&e))?;

            parse_text_response(resp, label).await
        }
    })
    .await
}

/// Little-endian 16-bit PCM bytes for `pcm_s16le` / `linear16` audio frames.
/// Shared by Soniox RT and Deepgram RT WebSocket streaming.
pub(super) fn samples_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{get_validate, openai_compatible_transcribe, warm_origin, AuthScheme, SttError};
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tempfile::NamedTempFile;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    #[test]
    fn transcription_prompt_capped_to_token_budget_on_term_boundary() {
        // Short prompts pass through untouched.
        let short = "Preferred spellings: shadcn/ui, Tauri.";
        assert_eq!(super::cap_transcription_prompt(short), short);

        // A large dictionary is capped (OpenAI/Groq honor only the final ~224
        // tokens) on a separator, so whole leading terms survive with no
        // mid-token cut.
        let many: Vec<String> = (0..300).map(|i| format!("term{i}")).collect();
        let long = format!("Preferred spellings: {}.", many.join(", "));
        let capped = super::cap_transcription_prompt(&long);
        assert!(capped.len() <= 672, "capped len was {}", capped.len());
        assert!(!capped.ends_with(',') && !capped.ends_with(' '));
        assert!(long.starts_with(&capped));
    }

    fn audio_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"wav").unwrap();
        file
    }

    async fn mount_sequence(server: &MockServer, responses: Vec<ResponseTemplate>) {
        let counter = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(responses);
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(move |_request: &Request| {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                responses
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| responses.last().unwrap().clone())
            })
            .expect(2)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_posts_multipart_and_parses_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "hello"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let text = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        assert_eq!(text, "hello");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        let content_length = request
            .headers
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(!content_length.is_empty());
        let content_type = request
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        let body = String::from_utf8_lossy(&request.body);
        assert!(body.contains("name=\"model\""));
        assert!(body.contains("gpt-4o-transcribe"));
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_gpt_transcribe_sends_languages_array_not_singular() {
        // `gpt-transcribe` expects the language as `languages[]`; the singular
        // `language` field would be rejected by the API.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "ok"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-transcribe",
            audio.path(),
            Some("en"),
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        let requests = server.received_requests().await.unwrap();
        let body = String::from_utf8_lossy(&requests[0].body);
        assert!(body.contains("name=\"languages[]\""));
        assert!(body.contains("\r\n\r\nen\r\n"));
        assert!(!body.contains("name=\"language\""));
        // The selected model must be propagated, not a hardcoded default.
        assert!(body.contains("name=\"model\""));
        assert!(body.contains("gpt-transcribe"));
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_keeps_singular_language_for_other_models() {
        // Every other model (OpenAI mini, Groq whisper) keeps `language`.
        for model in ["gpt-4o-mini-transcribe", "whisper-large-v3-turbo"] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/audio/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "text": "ok"
                })))
                .expect(1)
                .mount(&server)
                .await;
            let audio = audio_file();

            openai_compatible_transcribe(
                &server.uri(),
                "k",
                model,
                audio.path(),
                Some("en"),
                None,
                "transcription",
            )
            .await
            .unwrap();

            let requests = server.received_requests().await.unwrap();
            let body = String::from_utf8_lossy(&requests[0].body);
            assert!(body.contains("name=\"language\""), "model {model}");
            assert!(!body.contains("name=\"languages[]\""), "model {model}");
            assert!(body.contains(model), "model {model} not propagated");
        }
    }
    #[tokio::test]
    async fn openai_compatible_transcribe_includes_prompt_field_when_provided() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "ok"
            })))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();
        let prompt = "Preferred spellings: Voicetypr, Tauri.";

        let text = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            Some(prompt),
            "OpenAI transcription",
        )
        .await
        .unwrap();

        assert_eq!(text, "ok");
        let request = &server.received_requests().await.unwrap()[0];
        let body = String::from_utf8_lossy(&request.body);
        assert!(body.contains("name=\"prompt\""));
        assert!(body.contains("Preferred spellings: Voicetypr, Tauri."));
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_omits_prompt_field_when_empty_or_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "ok"
            })))
            .expect(2)
            .mount(&server)
            .await;
        let audio = audio_file();

        // Empty/whitespace prompt must not produce a `prompt` part.
        openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            Some("   "),
            "OpenAI transcription",
        )
        .await
        .unwrap();
        // `None` prompt must not produce a `prompt` part either.
        openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        for request in server.received_requests().await.unwrap() {
            let body = String::from_utf8_lossy(&request.body);
            assert!(!body.contains("name=\"prompt\""));
        }
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_retries_500_once_then_succeeds() {
        let server = MockServer::start().await;
        mount_sequence(
            &server,
            vec![
                ResponseTemplate::new(500).set_body_string("temporary"),
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "text": "hello"
                })),
            ],
        )
        .await;
        let audio = audio_file();

        let text = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(text, "hello");
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert!(request.headers.get("content-length").is_some());
        }
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_reopens_audio_file_for_retry() {
        let server = MockServer::start().await;
        let audio = audio_file();
        let audio_path = audio.path().to_path_buf();
        let responses_seen = Arc::new(AtomicUsize::new(0));
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(move |_request: &Request| {
                if responses_seen.fetch_add(1, Ordering::SeqCst) == 0 {
                    std::fs::write(&audio_path, b"two").unwrap();
                    ResponseTemplate::new(500).set_body_string("temporary")
                } else {
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "text": "hello"
                    }))
                }
            })
            .expect(2)
            .mount(&server)
            .await;

        let text = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap();

        let requests = server.received_requests().await.unwrap();
        let second_body = String::from_utf8_lossy(&requests[1].body);
        assert_eq!(text, "hello");
        assert!(second_body.contains("two"));
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_retries_500_once_then_returns_server_without_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("unique-provider-body"))
            .expect(2)
            .mount(&server)
            .await;
        let audio = audio_file();

        let error = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SttError::Server));
        let message = error.message("OpenAI");
        assert!(!message.contains("unique-provider-body"));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn openai_compatible_transcribe_does_not_retry_auth_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/audio/transcriptions"))
            .respond_with(ResponseTemplate::new(401).set_body_string("bad key"))
            .expect(1)
            .mount(&server)
            .await;
        let audio = audio_file();

        let error = openai_compatible_transcribe(
            &server.uri(),
            "k",
            "gpt-4o-transcribe",
            audio.path(),
            None,
            None,
            "OpenAI transcription",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SttError::Auth));
        assert_eq!(error.message("OpenAI"), "Invalid OpenAI API key");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_validate_succeeds_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .and(header("authorization", "Bearer k"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        get_validate(
            &format!("{}/models", server.uri()),
            AuthScheme::Bearer,
            "k",
            "Groq",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn get_validate_maps_auth_error_without_retry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;

        let error = get_validate(
            &format!("{}/models", server.uri()),
            AuthScheme::Bearer,
            "k",
            "Groq",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SttError::Auth));
        assert_eq!(error.message("Groq"), "Invalid Groq API key");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_validate_retries_500_once_then_returns_server() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(500))
            .expect(2)
            .mount(&server)
            .await;

        let error = get_validate(
            &format!("{}/models", server.uri()),
            AuthScheme::Bearer,
            "k",
            "Groq",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, SttError::Server));
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
    #[test]
    fn http_request_timeout_never_preempts_executor_budget() {
        // The executor clamps its cloud watchdog budget to [180s, 30 min] via
        // transcription_watchdog_budget. The HTTP client's total request
        // timeout must be >= that ceiling so a legitimately long upload is
        // bounded by the executor's shared deadline — never killed early by
        // reqwest (the previous 120s cap dropped long jobs as SttError::Timeout
        // before the budget elapsed).
        let max_budget = crate::commands::audio::transcription_watchdog_budget(Some(u64::MAX));
        assert!(
            super::REQUEST_TIMEOUT >= max_budget,
            "HTTP request timeout ({:?}) must be >= the max executor budget ({max_budget:?})",
            super::REQUEST_TIMEOUT
        );
        // And it must remain a real, generous finite deadline for direct callers.
        assert!(super::REQUEST_TIMEOUT.as_secs() >= 30 * 60);
    }

    #[tokio::test]
    async fn get_validate_bounded_by_short_deadline_not_transcription_budget() {
        // Regression: validation used the shared 30-min transcription client,
        // so a connected-but-unresponsive validation endpoint held the
        // validation command for ~1 h (`with_retry` doubles the budget on the
        // transient Timeout). Validation must now use a short, interactive
        // deadline.
        //
        // We shrink that deadline via the test seam and point get_validate at a
        // server that accepts the connection but delays its answer far past the
        // deadline. The call must surface SttError::Timeout well within the
        // transcription budget. If get_validate were reverted to http_client()
        // (30-min client) this would hang until the 5 s outer timeout fails the
        // test — so this genuinely guards the fix.
        super::set_validate_timeout_override(Some(500));
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(3)))
            .expect(2) // a transient Timeout is retried once
            .mount(&server)
            .await;

        let start = Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            get_validate(
                &format!("{}/models", server.uri()),
                AuthScheme::Bearer,
                "k",
                "Groq",
            ),
        )
        .await;
        super::set_validate_timeout_override(None);

        let elapsed = start.elapsed();
        let err = outcome
            .expect("validation must return under the short deadline, not hang")
            .unwrap_err();
        assert!(matches!(err, SttError::Timeout), "got {err:?}");
        // Two attempts (500 ms deadline each) plus a 400 ms backoff land far
        // below the 30-min transcription budget.
        assert!(
            elapsed < Duration::from_secs(5),
            "validation took {elapsed:?}; the short deadline is not in effect"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn warm_origin_sends_head_and_is_nonfatal_on_error() {
        // 200: the HEAD goes out on the shared client and completes (not a no-op).
        let server = MockServer::start().await;
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        warm_origin(&server.uri()).await;
        assert_eq!(server.received_requests().await.unwrap().len(), 1);

        // 500: fire-and-forget swallows the non-2xx; still exactly one request.
        let server = MockServer::start().await;
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&server)
            .await;
        warm_origin(&server.uri()).await;
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[test]
    fn samples_encode_as_little_endian_s16() {
        // 1 == 0x0001 -> [0x01, 0x00]; -1 == 0xFFFF -> [0xFF, 0xFF]; 256 -> [0x00, 0x01].
        assert_eq!(
            super::samples_to_le_bytes(&[1, -1, 256]),
            vec![1, 0, 255, 255, 0, 1]
        );
        assert!(super::samples_to_le_bytes(&[]).is_empty());
    }

    #[test]
    fn soniox_429_subcauses_split_rate_from_storage_quota() {
        // Actual documented JSON shapes (docs "Errors" reference): the
        // per-minute rate example must stay transient, the retained-storage
        // examples classify file-storage, the stored-transcription count
        // classifies record-capacity.
        let rate_rpm = r#"{"status_code":429,"error_type":"limit_exceeded","message":"Requests per minute limit for async transcription has been exceeded for your organization."}"#;
        assert_eq!(super::soniox_limit_is_file_storage(rate_rpm), None);

        let rate_file_mgmt = r#"{"status_code":429,"error_type":"limit_exceeded","message":"Requests per minute limit for file management has been exceeded for your organization."}"#;
        assert_eq!(super::soniox_limit_is_file_storage(rate_file_mgmt), None);

        let storage_files = r#"{"status_code":429,"error_type":"limit_exceeded","message":"Total file count limit has been exceeded for your organization. Please delete some."}"#;
        assert_eq!(
            super::soniox_limit_is_file_storage(storage_files),
            Some(true)
        );

        // Documented limit identifiers must classify even when the wording
        // changes around them.
        let storage_slug = r#"{"error_type":"limit_exceeded","message":"Adding this file would put the organization over its files_total_size_gb cap."}"#;
        assert_eq!(
            super::soniox_limit_is_file_storage(storage_slug),
            Some(true)
        );

        let storage_transcriptions = r#"{"error_type":"limit_exceeded","message":"transcribe_async_total_num_files limit has been exceeded."}"#;
        assert_eq!(
            super::soniox_limit_is_file_storage(storage_transcriptions),
            Some(false)
        );

        // The transient "pending file count" wall mentions "file count" but
        // not a retained-storage cap — it must stay transient.
        let pending = r#"{"error_type":"limit_exceeded","message":"Pending file count limit has been exceeded. Wait for in-flight transcriptions to complete."}"#;
        assert_eq!(super::soniox_limit_is_file_storage(pending), None);

        // Unknown 429 wording stays transient — never a destructive cleanup.
        let unknown =
            r#"{"error_type":"limit_exceeded","message":"Some future limit you have never seen."}"#;
        assert_eq!(super::soniox_limit_is_file_storage(unknown), None);

        // Different error_type / non-JSON / missing message: not a quota wall.
        assert_eq!(
            super::soniox_limit_is_file_storage(
                r#"{"error_type":"unauthenticated","message":"Total file count limit exceeded"}"#
            ),
            None
        );
        assert_eq!(super::soniox_limit_is_file_storage("not json at all"), None);
        assert_eq!(
            super::soniox_limit_is_file_storage(r#"{"error_type":"limit_exceeded"}"#),
            None
        );
    }
    #[tokio::test]
    async fn other_provider_429_does_not_trigger_soniox_cleanup_classification() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error_type":"limit_exceeded", "message":"Total file count limit has been exceeded"
            })))
            .mount(&server)
            .await;
        let response = reqwest::get(server.uri()).await.unwrap();
        assert!(matches!(
            super::log_http_body(response, "Deepgram").await,
            SttError::RateLimited
        ));
    }

    #[tokio::test]
    async fn log_http_body_maps_soniox_429_json_paths_to_typed_quota_kinds() {
        for (body, expected_label, expected_storage) in [
            (
                r#"{"status_code":429,"error_type":"limit_exceeded","message":"Requests per minute limit for file management has been exceeded for your organization."}"#,
                "RateLimited",
                None,
            ),
            (
                r#"{"status_code":429,"error_type":"limit_exceeded","message":"Total file count limit has been exceeded for your organization. Please delete some."}"#,
                "LimitExceeded",
                Some(true),
            ),
            (
                r#"{"status_code":429,"error_type":"limit_exceeded","message":"transcribe_async_total_num_files limit has been exceeded."}"#,
                "LimitExceeded",
                Some(false),
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(429).set_body_string(body))
                .expect(1)
                .mount(&server)
                .await;
            let resp = reqwest::get(server.uri()).await.unwrap();
            let err = super::log_soniox_http_body(resp, "Soniox classification").await;
            // The file_storage FIELD is part of the contract: a
            // discriminant-only compare could pass {true} vs {false}.
            let kind = match &err {
                SttError::RateLimited => ("RateLimited", None),
                SttError::LimitExceeded { file_storage } => ("LimitExceeded", Some(*file_storage)),
                other => unreachable!("unexpected classification {other:?}"),
            };
            assert_eq!(kind, (expected_label, expected_storage));
        }
    }
}

#[cfg(test)]
mod safe_log_tests {
    use super::*;
    #[test]
    fn http_log_contains_only_closed_provider_status_and_classification() {
        assert_eq!(
            http_failure_log(
                "Soniox upload private words",
                429,
                &SttError::LimitExceeded { file_storage: true }
            ),
            "provider=Soniox status=429 code=cloud_storage_limit"
        );
        assert_eq!(
            http_failure_log("private response /Users/alice key", 500, &SttError::Server),
            "provider=unknown status=500 code=unavailable"
        );
    }
}
