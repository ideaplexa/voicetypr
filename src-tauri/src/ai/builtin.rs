//! Built-in Polish: for a user who has not connected an AI provider of their
//! own, Voicetypr's server cleans up the dictation during an active trial or
//! with a licence. Text leaves the device only while the user's
//! `builtin_polish_enabled` setting is on. Any failure returns an error the
//! writing pipeline turns into the engine's own text, without a notice.
//!
//! Before any trial or licence, onboarding's First try may be polished
//! through a small allowance of its own (`/polish/onboarding`, capped per
//! device on the server). Only `recording/onboarding_try.rs` asks for it, for
//! the try it owns; the dictation pipeline never does.
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
    setting_on && !own_provider_ready && trial_or_licence(licence)
}

fn trial_or_licence(licence: &RuntimeLicenseCache) -> bool {
    matches!(
        licence,
        RuntimeLicenseCache::Ready(cached)
            if matches!(cached.status.status, LicenseState::Trial | LicenseState::Licensed)
    )
}

/// Onboarding's First try before a trial or licence, with the switch on and no
/// provider of the user's own; a trial or licence uses the normal route.
pub(crate) fn eligible_first_try(
    setting_on: bool,
    own_provider_ready: bool,
    licence: &RuntimeLicenseCache,
) -> bool {
    setting_on
        && !own_provider_ready
        && !trial_or_licence(licence)
        && !matches!(licence, RuntimeLicenseCache::Loading)
}

/// A fresh install, whose settings never recorded onboarding, starts with
/// built-in Polish on; an existing install keeps it off until the user chooses.
pub(crate) fn default_on_for_fresh_install<R: tauri::Runtime>(
    store: &tauri_plugin_store::Store<R>,
) {
    if store.get("onboarding_completed").is_none() && store.get(SETTING_KEY).is_none() {
        store.set(SETTING_KEY, serde_json::json!(true));
    }
}

/// Runs `work` while the First try is still current; dropping it cancels the
/// request, so nothing more goes out once the try ends.
async fn while_current<T>(
    still_current: &impl Fn() -> bool,
    work: impl std::future::Future<Output = T>,
) -> Option<T> {
    let ended = async {
        while still_current() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    };
    tokio::select! {
        out = work => Some(out),
        () = ended => None,
    }
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
    keep_words: bool,
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
    route: &str,
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
        .post(format!("{base_url}/{route}"))
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

/// Whether built-in Polish would clean the next dictation if its switch is on.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Availability {
    /// The user's own AI provider is set up, so it is used instead.
    OwnProvider,
    /// No trial or licence on this device yet.
    NoAccess,
    /// No trial or licence yet, but onboarding's first try may use its small
    /// allowance; nothing is asked of the server for this answer.
    FirstTry,
    /// Voicetypr's server confirmed it will clean this device's dictation.
    Ready,
    /// The server refused this device, such as a trial started without an email.
    NotEntitled,
    /// The server could not be reached or is not taking Polish right now.
    Unreachable,
}

fn availability_from_status(status: u16) -> Availability {
    match status {
        200..=299 => Availability::Ready,
        403 => Availability::NotEntitled,
        _ => Availability::Unreachable,
    }
}

/// A confirmed "ready" is trusted for this long, so the Polish step and Settings
/// don't ask the server on every render; a refusal or an outage is asked again.
const READY_FOR: Duration = Duration::from_secs(10 * 60);

/// Asks Voicetypr's server whether it will Polish for this device, sending only
/// the device hash and the licence key, never text. Only asked once this
/// device has a trial or a licence.
pub(crate) async fn availability(app: &AppHandle, own_provider_ready: bool) -> Availability {
    if own_provider_ready {
        return Availability::OwnProvider;
    }
    let Some(state) = app.try_state::<crate::AppState>() else {
        return Availability::NoAccess;
    };
    let licence_state = {
        let cache = state.license_cache.read().await;
        if !eligible(true, false, &cache) {
            return if onboarding_unfinished(app) && !matches!(&*cache, RuntimeLicenseCache::Loading)
            {
                Availability::FirstTry
            } else {
                Availability::NoAccess
            };
        }
        match &*cache {
            RuntimeLicenseCache::Ready(cached) => format!("{:?}", cached.status.status),
            _ => return Availability::NoAccess,
        }
    };
    let license_key = crate::license::keychain::get_license(app).ok().flatten();
    let entitlement = entitlement_fingerprint(&licence_state, license_key.as_deref());
    if confirmed(entitlement) {
        return Availability::Ready;
    }
    let Ok(device_hash) = device_hash() else {
        return Availability::Unreachable;
    };
    let base_url = crate::license::api_client::get_api_base_url();
    let answer = check(client(), &base_url, device_hash, license_key.as_deref()).await;
    if let Ok(mut slot) = CONFIRMED.lock() {
        *slot = (answer == Availability::Ready).then(|| (entitlement, Instant::now()));
    }
    answer
}

fn onboarding_unfinished(app: &AppHandle) -> bool {
    app.store("settings")
        .ok()
        .and_then(|store| store.get("onboarding_completed"))
        .and_then(|value| value.as_bool())
        != Some(true)
}

/// The last server confirmation, bound to the licence state and key it was given for.
static CONFIRMED: std::sync::Mutex<Option<(u64, Instant)>> = std::sync::Mutex::new(None);

fn entitlement_fingerprint(licence_state: &str, license_key: Option<&str>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (licence_state, license_key).hash(&mut hasher);
    hasher.finish()
}

fn confirmed(entitlement: u64) -> bool {
    CONFIRMED
        .lock()
        .ok()
        .and_then(|slot| *slot)
        .is_some_and(|(confirmed_for, at)| confirmed_for == entitlement && at.elapsed() < READY_FOR)
}

/// A refusal from the server ends the confirmation, so the next dictation asks again first.
fn forget_confirmation() {
    if let Ok(mut slot) = CONFIRMED.lock() {
        *slot = None;
    }
}

async fn check(
    client: &reqwest::Client,
    base_url: &str,
    device_hash: &str,
    license_key: Option<&str>,
) -> Availability {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct StatusRequest<'a> {
        device_hash: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        license_key: Option<&'a str>,
    }
    #[derive(Deserialize)]
    struct StatusEnvelope {
        data: Option<StatusData>,
    }
    #[derive(Deserialize)]
    struct StatusData {
        available: bool,
    }
    let Ok(response) = client
        .post(format!("{base_url}/polish/status"))
        .timeout(Duration::from_secs(5))
        .json(&StatusRequest {
            device_hash,
            license_key,
        })
        .send()
        .await
    else {
        return Availability::Unreachable;
    };
    let answer = availability_from_status(response.status().as_u16());
    if answer != Availability::Ready {
        return answer;
    }
    match response.json::<StatusEnvelope>().await {
        Ok(StatusEnvelope {
            data: Some(StatusData { available: true }),
        }) => Availability::Ready,
        _ => Availability::Unreachable,
    }
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
    let request = PolishRequest {
        device_hash,
        license_key: license_key.as_deref(),
        text,
        style,
        language: language_code(transcript_language),
        keep_words: crate::ai::keep_words::read(app),
    };
    // A cancelled or superseded recording never sends its text, and stops waiting at once.
    let cancellation = crate::commands::ai_lifecycle::polish_cancellation();
    let base_url = crate::license::api_client::get_api_base_url();
    let started = Instant::now();
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(AiProviderError::Canceled),
        result = async {
            let switch_on = app
                .store("settings")
                .map(|store| setting_on(&store))
                .unwrap_or(false);
            if !switch_on {
                return Err(AiProviderError::Canceled);
            }
            // Text goes out only after the server has confirmed this device's entitlement.
            if availability(app, false).await != Availability::Ready {
                return Err(AiProviderError::ServiceUnavailable);
            }
            let result = post(client(), &base_url, "polish", &request).await;
            if matches!(result, Err(AiProviderError::ServiceUnavailable)) {
                forget_confirmation();
            }
            result
        } => result,
    }
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

/// Who is asking, for a First try: read outside the settings store.
pub(crate) struct FirstTryIdentity<'a> {
    pub(crate) device_hash: &'a str,
    pub(crate) license_key: Option<&'a str>,
    pub(crate) own_provider_ready: bool,
}

/// Cleans onboarding's First try for the setup window alone: nothing is pasted
/// or saved. `still_current` is the try's own gate; once it closes, nothing is
/// sent and no text comes back. None means the try shows the engine's text.
pub(crate) async fn polish_first_try(
    app: &AppHandle,
    text: &str,
    still_current: impl Fn() -> bool,
) -> Option<String> {
    let device_hash = device_hash().ok()?;
    let license_key = crate::license::keychain::get_license(app).ok().flatten();
    let identity = FirstTryIdentity {
        device_hash,
        license_key: license_key.as_deref(),
        own_provider_ready: crate::commands::ai::has_ai_model_and_key(app).unwrap_or(true),
    };
    let base_url = crate::license::api_client::get_api_base_url();
    first_try_with(app, client(), &base_url, identity, text, still_current).await
}

pub(crate) async fn first_try_with<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    client: &reqwest::Client,
    base_url: &str,
    identity: FirstTryIdentity<'_>,
    text: &str,
    still_current: impl Fn() -> bool,
) -> Option<String> {
    let store = app.store("settings").ok()?;
    let switch_on = || setting_on(&store);
    if !switch_on() || identity.own_provider_ready || text.trim().is_empty() {
        return None;
    }
    let entitled = {
        let state = app.try_state::<crate::AppState>()?;
        let cache = state.license_cache.read().await;
        if trial_or_licence(&cache) {
            true
        } else if eligible_first_try(true, false, &cache) {
            false
        } else {
            return None;
        }
    };
    let preset = crate::ai::prompts::enhancement_options_for_ai_enabled(
        store.get("enhancement_options").as_ref(),
        true,
    )
    .map(|options| options.preset)
    .unwrap_or(EnhancementPreset::CleanDictation);
    let language = store
        .get("speech_language")
        .and_then(|value| value.as_str().map(str::to_owned));
    let keep_words = crate::ai::keep_words::enabled(store.get("polish_keep_words").as_ref(), false);
    let request = PolishRequest {
        device_hash: identity.device_hash,
        license_key: identity.license_key.filter(|_| entitled),
        text,
        style: style(preset).unwrap_or("clean"),
        language: language_code(language.as_deref()),
        keep_words,
    };
    if !still_current() {
        return None;
    }
    if entitled
        && while_current(
            &still_current,
            check(client, base_url, identity.device_hash, identity.license_key),
        )
        .await?
            != Availability::Ready
    {
        return None;
    }
    if !still_current() || !switch_on() {
        return None;
    }
    let route = if entitled {
        "polish"
    } else {
        "polish/onboarding"
    };
    let started = Instant::now();
    let result = while_current(&still_current, post(client, base_url, route, &request)).await?;
    let output = result.ok().map(|data| data.text.trim().to_string());
    let output = output.filter(|output| crate::ai::output_guard::check(text, output).is_ok());
    log::info!(
        "First try Polish {} (input chars: {}, duration_ms: {})",
        if output.is_some() {
            "applied"
        } else {
            "unavailable"
        },
        text.chars().count(),
        started.elapsed().as_millis()
    );
    output.filter(|_| still_current())
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
    fn the_first_try_allowance_is_only_before_any_trial_or_licence() {
        let none = ready(LicenseState::None);
        assert!(eligible_first_try(true, false, &none));
        assert!(eligible_first_try(
            true,
            false,
            &RuntimeLicenseCache::Failed
        ));
        assert!(
            !eligible_first_try(false, false, &none),
            "the off switch sends nothing"
        );
        assert!(
            !eligible_first_try(true, true, &none),
            "the user's own provider wins"
        );
        assert!(
            !eligible_first_try(true, false, &RuntimeLicenseCache::Loading),
            "as polish_status says while the licence loads"
        );
        for access in [LicenseState::Trial, LicenseState::Licensed] {
            assert!(
                !eligible_first_try(true, false, &ready(access)),
                "a trial or licence uses the normal route"
            );
        }
    }

    fn mock_app(dir: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = dir.to_str().unwrap().to_string();
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::default().build())
            .build(context)
            .unwrap();
        app.manage(crate::AppState::new());
        app.store("settings")
            .unwrap()
            .set(SETTING_KEY, serde_json::json!(true));
        app
    }

    async fn licence(app: &tauri::App<tauri::test::MockRuntime>, cache: RuntimeLicenseCache) {
        *app.state::<crate::AppState>().license_cache.write().await = cache;
    }

    const HEARD: &str = "um so let's move the sync to thursday no friday at three";
    const CLEANED: &str = "Let's move the sync to Friday at three.";

    fn identity(own_provider_ready: bool) -> FirstTryIdentity<'static> {
        FirstTryIdentity {
            device_hash: "abc",
            license_key: Some("KEY"),
            own_provider_ready,
        }
    }

    fn cleaned_reply() -> ResponseTemplate {
        ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({ "success": true, "data": { "text": CLEANED } }))
    }

    #[tokio::test]
    async fn a_first_try_before_any_trial_is_cleaned_on_the_onboarding_route_and_leaves_no_history()
    {
        let dir = tempfile::tempdir().unwrap();
        let app = mock_app(dir.path());
        licence(&app, ready(LicenseState::None)).await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/polish/onboarding"))
            .and(body_json(serde_json::json!({
                "deviceHash": "abc",
                "text": HEARD,
                "style": "clean",
                "keepWords": false
            })))
            .respond_with(cleaned_reply())
            .expect(1)
            .mount(&server)
            .await;

        let cleaned = first_try_with(
            app.handle(),
            &reqwest::Client::new(),
            &server.uri(),
            identity(false),
            HEARD,
            || true,
        )
        .await;

        assert_eq!(cleaned.as_deref(), Some(CLEANED));
        assert!(
            app.store("transcriptions").unwrap().keys().is_empty(),
            "a First try is never a History row"
        );
        assert!(!dir.path().join("transcriptions").exists());
    }

    #[tokio::test]
    async fn a_first_try_with_a_trial_uses_the_normal_route_after_the_servers_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let app = mock_app(dir.path());
        licence(&app, ready(LicenseState::Trial)).await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/polish/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({ "success": true, "data": { "available": true } }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/polish"))
            .and(body_json(serde_json::json!({
                "deviceHash": "abc",
                "licenseKey": "KEY",
                "text": HEARD,
                "style": "clean",
                "keepWords": false
            })))
            .respond_with(cleaned_reply())
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/polish/onboarding"))
            .respond_with(cleaned_reply())
            .expect(0)
            .mount(&server)
            .await;

        let cleaned = first_try_with(
            app.handle(),
            &reqwest::Client::new(),
            &server.uri(),
            identity(false),
            HEARD,
            || true,
        )
        .await;
        assert_eq!(cleaned.as_deref(), Some(CLEANED));
    }

    #[tokio::test]
    async fn a_first_try_sends_nothing_when_off_owned_ended_or_still_loading() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(cleaned_reply())
            .expect(0)
            .mount(&server)
            .await;
        let client = reqwest::Client::new();

        let dir = tempfile::tempdir().unwrap();
        let app = mock_app(dir.path());
        licence(&app, ready(LicenseState::None)).await;
        app.store("settings")
            .unwrap()
            .set(SETTING_KEY, serde_json::json!(false));
        let off = first_try_with(
            app.handle(),
            &client,
            &server.uri(),
            identity(false),
            HEARD,
            || true,
        )
        .await;
        assert_eq!(off, None, "switched off");

        let dir = tempfile::tempdir().unwrap();
        let app = mock_app(dir.path());
        licence(&app, ready(LicenseState::None)).await;
        let own = first_try_with(
            app.handle(),
            &client,
            &server.uri(),
            identity(true),
            HEARD,
            || true,
        )
        .await;
        assert_eq!(own, None, "the user's own provider");
        let ended = first_try_with(
            app.handle(),
            &client,
            &server.uri(),
            identity(false),
            HEARD,
            || false,
        )
        .await;
        assert_eq!(ended, None, "the try ended before the send");
        licence(&app, RuntimeLicenseCache::Loading).await;
        let loading = first_try_with(
            app.handle(),
            &client,
            &server.uri(),
            identity(false),
            HEARD,
            || true,
        )
        .await;
        assert_eq!(loading, None, "the licence is still loading");
    }

    #[tokio::test]
    async fn a_first_try_that_ends_while_its_request_is_out_gets_nothing_back() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/polish/onboarding"))
            .respond_with(cleaned_reply().set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let app = mock_app(dir.path());
        licence(&app, ready(LicenseState::None)).await;
        let started = Instant::now();
        let cleaned = first_try_with(
            app.handle(),
            &reqwest::Client::new(),
            &server.uri(),
            identity(false),
            HEARD,
            || started.elapsed() < Duration::from_millis(200),
        )
        .await;
        assert_eq!(cleaned, None);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the request is dropped when the try ends"
        );
    }

    #[test]
    fn only_a_fresh_install_starts_with_built_in_polish_on() {
        let dir = tempfile::tempdir().unwrap();
        let app = mock_app(dir.path());
        let store = app.store("settings").unwrap();
        store.clear();
        default_on_for_fresh_install(&store);
        assert_eq!(store.get(SETTING_KEY), Some(serde_json::json!(true)));

        for (completed, choice) in [(false, None), (true, None), (false, Some(false))] {
            store.clear();
            store.set("onboarding_completed", serde_json::json!(completed));
            if let Some(choice) = choice {
                store.set(SETTING_KEY, serde_json::json!(choice));
            }
            default_on_for_fresh_install(&store);
            assert_eq!(
                store.get(SETTING_KEY),
                choice.map(|on| serde_json::json!(on))
            );
        }
        store.clear();
        store.set(SETTING_KEY, serde_json::json!(false));
        default_on_for_fresh_install(&store);
        assert_eq!(store.get(SETTING_KEY), Some(serde_json::json!(false)));
    }

    #[tokio::test]
    async fn a_used_up_allowance_or_an_ended_try_keeps_the_engines_text() {
        let dir = tempfile::tempdir().unwrap();
        let app = mock_app(dir.path());
        licence(&app, ready(LicenseState::None)).await;
        let used_up = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).set_body_json(
                serde_json::json!({ "success": false, "error": "polish_onboarding_used_up" }),
            ))
            .mount(&used_up)
            .await;
        let client = reqwest::Client::new();
        assert_eq!(
            first_try_with(
                app.handle(),
                &client,
                &used_up.uri(),
                identity(false),
                HEARD,
                || true
            )
            .await,
            None
        );

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(cleaned_reply())
            .mount(&server)
            .await;
        let calls = std::sync::atomic::AtomicU32::new(0);
        let ends_during_the_send = || calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
        assert_eq!(
            first_try_with(
                app.handle(),
                &client,
                &server.uri(),
                identity(false),
                HEARD,
                ends_during_the_send
            )
            .await,
            None,
            "a try that ended while Polish ran gets no text"
        );
    }

    #[tokio::test]
    async fn the_first_try_goes_to_the_onboarding_route_without_a_licence_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/polish/onboarding"))
            .and(body_json(serde_json::json!({
                "deviceHash": "abc",
                "text": "um so tuesday no wednesday",
                "style": "clean",
                "language": "en",
                "keepWords": false
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true, "data": {"text": "Wednesday."}
            })))
            .mount(&server)
            .await;
        let data = post(
            &reqwest::Client::new(),
            &server.uri(),
            "polish/onboarding",
            &request(),
        )
        .await
        .unwrap();
        assert_eq!(data.text, "Wednesday.");

        let used_up = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "success": false, "error": "polish_onboarding_used_up"
            })))
            .mount(&used_up)
            .await;
        let error = post(
            &reqwest::Client::new(),
            &used_up.uri(),
            "polish/onboarding",
            &request(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(
            error,
            AiProviderError::RateLimited,
            "used up falls back to the engine's text"
        );
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
            keep_words: false,
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
                "language": "en",
                "keepWords": false
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "data": {"text": "Wednesday.", "model": "@cf/google/gemma-4-26b-a4b-it"}
            })))
            .mount(&server)
            .await;

        let data = post(&reqwest::Client::new(), &server.uri(), "polish", &request())
            .await
            .unwrap();
        assert_eq!(data.text, "Wednesday.");
    }

    #[tokio::test]
    async fn every_server_refusal_maps_to_a_polish_error() {
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
            let error = post(&reqwest::Client::new(), &server.uri(), "polish", &request())
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
            let error = post(&reqwest::Client::new(), &server.uri(), "polish", &request())
                .await
                .err()
                .unwrap();
            assert_eq!(error, AiProviderError::BadResponse);
        }
    }

    #[tokio::test]
    async fn the_status_check_sends_no_text_and_reads_the_servers_answer() {
        for (status, expected) in [
            (200, Availability::Ready),
            (403, Availability::NotEntitled),
            (503, Availability::Unreachable),
            (429, Availability::Unreachable),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/polish/status"))
                .and(body_json(
                    serde_json::json!({ "deviceHash": "abc", "licenseKey": "KEY" }),
                ))
                .respond_with(ResponseTemplate::new(status).set_body_json(
                    serde_json::json!({ "success": status == 200, "data": { "available": true } }),
                ))
                .mount(&server)
                .await;
            let answer = check(&reqwest::Client::new(), &server.uri(), "abc", Some("KEY")).await;
            assert_eq!(answer, expected, "status {status}");
        }
        let offline = check(&reqwest::Client::new(), "http://127.0.0.1:9", "abc", None).await;
        assert_eq!(offline, Availability::Unreachable);

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>proxy</html>"))
            .mount(&server)
            .await;
        let unexpected = check(&reqwest::Client::new(), &server.uri(), "abc", None).await;
        assert_eq!(
            unexpected,
            Availability::Unreachable,
            "a 200 without the answer is not ready"
        );
    }

    #[test]
    fn a_confirmation_belongs_to_the_licence_it_was_given_for() {
        let trial = entitlement_fingerprint("Trial", None);
        assert_ne!(trial, entitlement_fingerprint("Licensed", Some("KEY")));
        assert_ne!(
            entitlement_fingerprint("Licensed", Some("KEY")),
            entitlement_fingerprint("Licensed", Some("OTHER"))
        );
        *CONFIRMED.lock().unwrap() = Some((trial, Instant::now()));
        assert!(confirmed(trial));
        assert!(!confirmed(entitlement_fingerprint("Licensed", Some("KEY"))));
        forget_confirmation();
        assert!(!confirmed(trial));
    }

    #[tokio::test]
    async fn an_unreachable_server_is_a_network_error() {
        let error = post(
            &reqwest::Client::new(),
            "http://127.0.0.1:9",
            "polish",
            &request(),
        )
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
        let error = post(&reqwest::Client::new(), &server.uri(), "polish", &request())
            .await
            .err()
            .unwrap();
        assert_eq!(error, AiProviderError::Timeout);
        assert!(started.elapsed() < TIMEOUT + Duration::from_secs(2));
    }
}
