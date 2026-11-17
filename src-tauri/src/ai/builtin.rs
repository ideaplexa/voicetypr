//! Built-in Polish: for a user who has not connected an AI provider of their
//! own, Voicetypr's server cleans up the dictation during an active trial or
//! with a licence. Text leaves the device only while the user's
//! `builtin_polish_enabled` setting is on. Any failure returns an error the
//! writing pipeline turns into the engine's own text, without a notice.
use crate::ai::contract::AiPolishResult;
use crate::ai::error::AiProviderError;
use crate::ai::prompts::EnhancementPreset;
use crate::commands::ai::AiPolishAttemptError;
use crate::commands::license::RuntimeLicenseCache;
use crate::license::LicenseState;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

pub(crate) const SETTING_KEY: &str = "builtin_polish_enabled";
pub(crate) const PROVIDER_ID: &str = "voicetypr";
/// Below the 30 s budget of a user's own HTTP provider; the server gives up
/// on its upstream after 8 s.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Built-in Polish runs only when the user left it on, has no provider of
/// their own and the licence check last answered with a trial or a licence.
pub(crate) fn eligible(
    setting_on: bool,
    own_provider_ready: bool,
    licence: &RuntimeLicenseCache,
) -> bool {
    setting_on
        && !own_provider_ready
        && matches!(
            licence,
            RuntimeLicenseCache::Ready(cached)
                if matches!(cached.status.status, LicenseState::Trial | LicenseState::Licensed)
        )
}

pub(crate) fn setting_on<R: tauri::Runtime>(store: &tauri_plugin_store::Store<R>) -> bool {
    store
        .get(SETTING_KEY)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// Whether the next dictation uses built-in Polish. A licence cache that is
/// being refreshed right now counts as not eligible, so that take pastes the
/// engine's text instead of waiting.
pub(crate) fn active(app: &AppHandle, own_provider_ready: bool) -> bool {
    let Ok(store) = app.store("settings") else {
        return false;
    };
    let Some(state) = app.try_state::<crate::AppState>() else {
        return false;
    };
    let setting = setting_on(&store);
    state
        .license_cache
        .try_read()
        .map(|cache| eligible(setting, own_provider_ready, &cache))
        .unwrap_or(false)
}

fn style(preset: EnhancementPreset) -> Option<&'static str> {
    match preset {
        EnhancementPreset::PersonalDictation => None,
        EnhancementPreset::CleanDictation => Some("clean"),
        EnhancementPreset::Writing => Some("writing"),
        EnhancementPreset::Notes => Some("notes"),
        EnhancementPreset::Message => Some("message"),
        EnhancementPreset::Code => Some("code"),
    }
}

fn language_code(language: Option<&str>) -> Option<String> {
    let base = language?.split(['-', '_']).next()?.to_ascii_lowercase();
    ((2..=3).contains(&base.len()) && base.bytes().all(|byte| byte.is_ascii_lowercase()))
        .then_some(base)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolishRequest<'a> {
    device_hash: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    license_key: Option<&'a str>,
    text: &'a str,
    style: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
}

#[derive(Deserialize)]
struct PolishEnvelope {
    data: Option<PolishData>,
}

#[derive(Deserialize)]
struct PolishData {
    text: String,
    #[serde(default)]
    model: String,
}

fn map_status(status: u16) -> AiProviderError {
    match status {
        429 => AiProviderError::RateLimited,
        504 => AiProviderError::Timeout,
        403 | 503 => AiProviderError::ServiceUnavailable,
        _ => AiProviderError::BadResponse,
    }
}

async fn post(
    client: &reqwest::Client,
    base_url: &str,
    request: &PolishRequest<'_>,
) -> Result<PolishData, AiProviderError> {
    let transport = |error: reqwest::Error| {
        if error.is_timeout() {
            AiProviderError::Timeout
        } else {
            AiProviderError::Network
        }
    };
    let response = client
        .post(format!("{base_url}/polish"))
        .timeout(TIMEOUT)
        .json(request)
        .send()
        .await
        .map_err(transport)?;
    if !response.status().is_success() {
        return Err(map_status(response.status().as_u16()));
    }
    let envelope: PolishEnvelope = response.json().await.map_err(|error| {
        if error.is_timeout() {
            AiProviderError::Timeout
        } else {
            AiProviderError::BadResponse
        }
    })?;
    envelope
        .data
        .filter(|data| !data.text.trim().is_empty())
        .ok_or(AiProviderError::BadResponse)
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

fn device_hash() -> Result<&'static str, String> {
    static HASH: OnceLock<String> = OnceLock::new();
    if let Some(hash) = HASH.get() {
        return Ok(hash);
    }
    let hash = crate::license::device::get_device_hash()?;
    Ok(HASH.get_or_init(|| hash))
}

pub(crate) async fn polish(
    app: &AppHandle,
    text: &str,
    preset: EnhancementPreset,
    transcript_language: Option<&str>,
    spoken_punctuation: bool,
) -> Result<AiPolishResult, AiPolishAttemptError> {
    let fail = |error| AiPolishAttemptError {
        error,
        provider_id: PROVIDER_ID.to_string(),
        model_id: String::new(),
    };
    let style = style(preset).ok_or_else(|| fail(AiProviderError::InvalidModel))?;
    if !spoken_punctuation && crate::ai::skip::should_skip(text, preset, transcript_language) {
        return Ok(AiPolishResult {
            output_text: text.to_string(),
            provider_id: PROVIDER_ID.to_string(),
            model_id: String::new(),
            duration_ms: 0,
            fast_mode_fallback: false,
        });
    }
    let device_hash = device_hash().map_err(|_| fail(AiProviderError::Internal))?;
    let license_key = crate::license::keychain::get_license(app).ok().flatten();
    let started = Instant::now();
    let result = post(
        client(),
        &crate::license::api_client::get_api_base_url(),
        &PolishRequest {
            device_hash,
            license_key: license_key.as_deref(),
            text,
            style,
            language: language_code(transcript_language),
        },
    )
    .await
    .and_then(|data| {
        let output = data.text.trim().to_string();
        crate::ai::output_guard::check(text, &output)
            .map(|()| (output, data.model))
            .map_err(AiProviderError::OutputGuard)
    });
    let duration_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok((output_text, model_id)) => {
            log::info!(
                "Built-in Polish applied (input chars: {}, output chars: {}, duration_ms: {})",
                text.chars().count(),
                output_text.chars().count(),
                duration_ms
            );
            Ok(AiPolishResult {
                output_text,
                provider_id: PROVIDER_ID.to_string(),
                model_id,
                duration_ms,
                fast_mode_fallback: false,
            })
        }
        Err(error) => {
            log::info!(
                "Built-in Polish unavailable ({}); pasting the engine's text (duration_ms: {})",
                crate::ai::polish::error_category(&error),
                duration_ms
            );
            Err(fail(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::license::CachedLicense;
    use crate::license::LicenseStatus;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ready(status: LicenseState) -> RuntimeLicenseCache {
        RuntimeLicenseCache::Ready(CachedLicense::new(LicenseStatus {
            status,
            trial_days_left: None,
            license_type: None,
            license_key: None,
            expires_at: None,
            verification_state: None,
            verification_expires_at: None,
        }))
    }

    #[test]
    fn used_only_without_an_own_provider_during_a_trial_or_with_a_licence() {
        assert!(eligible(true, false, &ready(LicenseState::Trial)));
        assert!(eligible(true, false, &ready(LicenseState::Licensed)));
        assert!(
            !eligible(true, true, &ready(LicenseState::Trial)),
            "the user's own provider always wins"
        );
        assert!(
            !eligible(false, false, &ready(LicenseState::Trial)),
            "the off switch keeps text on the device"
        );
        assert!(!eligible(true, false, &ready(LicenseState::Expired)));
        assert!(
            !eligible(true, false, &ready(LicenseState::None)),
            "nothing before the trial starts"
        );
        assert!(!eligible(true, false, &RuntimeLicenseCache::Loading));
        assert!(!eligible(true, false, &RuntimeLicenseCache::Failed));
    }

    #[test]
    fn personal_dictation_never_calls_the_server() {
        assert_eq!(style(EnhancementPreset::PersonalDictation), None);
        assert_eq!(style(EnhancementPreset::CleanDictation), Some("clean"));
    }

    #[test]
    fn only_plain_language_codes_are_sent() {
        assert_eq!(language_code(Some("en")), Some("en".to_string()));
        assert_eq!(language_code(Some("pt-BR")), Some("pt".to_string()));
        assert_eq!(language_code(Some("auto")), None);
        assert_eq!(language_code(None), None);
    }

    fn request<'a>() -> PolishRequest<'a> {
        PolishRequest {
            device_hash: "abc",
            license_key: None,
            text: "um so tuesday no wednesday",
            style: "clean",
            language: Some("en".to_string()),
        }
    }

    #[tokio::test]
    async fn sends_the_dictation_and_returns_the_cleaned_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/polish"))
            .and(body_json(serde_json::json!({
                "deviceHash": "abc",
                "text": "um so tuesday no wednesday",
                "style": "clean",
                "language": "en"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": {"text": "Wednesday.", "model": "@cf/google/gemma-4-26b-a4b-it"}
            })))
            .mount(&server)
            .await;

        let data = post(&reqwest::Client::new(), &server.uri(), &request())
            .await
            .unwrap();
        assert_eq!(data.text, "Wednesday.");
    }

    #[tokio::test]
    async fn every_refusal_is_an_error_the_pipeline_falls_back_from() {
        for (status, expected) in [
            (403, AiProviderError::ServiceUnavailable),
            (503, AiProviderError::ServiceUnavailable),
            (429, AiProviderError::RateLimited),
            (504, AiProviderError::Timeout),
            (502, AiProviderError::BadResponse),
            (413, AiProviderError::BadResponse),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(status).set_body_json(serde_json::json!({
                        "success": false, "error": "polish_unavailable"
                    })),
                )
                .mount(&server)
                .await;
            let error = post(&reqwest::Client::new(), &server.uri(), &request())
                .await
                .err()
                .unwrap();
            assert_eq!(error, expected, "status {status}");
        }
    }

    #[tokio::test]
    async fn empty_or_malformed_answers_are_errors() {
        for body in [
            serde_json::json!({"success": true, "data": {"text": "  "}}),
            serde_json::json!({"success": true}),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&server)
                .await;
            let error = post(&reqwest::Client::new(), &server.uri(), &request())
                .await
                .err()
                .unwrap();
            assert_eq!(error, AiProviderError::BadResponse);
        }
    }

    #[tokio::test]
    async fn an_unreachable_server_is_a_network_error() {
        let error = post(&reqwest::Client::new(), "http://127.0.0.1:9", &request())
            .await
            .err()
            .unwrap();
        assert_eq!(error, AiProviderError::Network);
    }

    #[tokio::test]
    async fn a_slow_server_is_cut_off_at_the_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data": {"text": "late"}}))
                    .set_delay(TIMEOUT + Duration::from_secs(5)),
            )
            .mount(&server)
            .await;
        let started = Instant::now();
        let error = post(&reqwest::Client::new(), &server.uri(), &request())
            .await
            .err()
            .unwrap();
        assert_eq!(error, AiProviderError::Timeout);
        assert!(started.elapsed() < TIMEOUT + Duration::from_secs(2));
    }
}
