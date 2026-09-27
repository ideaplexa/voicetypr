//! Consent-gated, personless product analytics (PostHog Cloud EU).
//!
//! This module is deliberately separate from `telemetry`: PostHog receives only
//! closed product events, while GlitchTip remains the sole owner of errors,
//! crashes, logs, traces, and symbolication. There is no frontend SDK,
//! autocapture, replay, identify call, feature-flag evaluation, or error tracking.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock};

use parking_lot::RwLock;

use posthog_rs::{Client, ClientOptionsBuilder, Event};
use serde_json::Value;

use crate::release_channel::RELEASE_CHANNEL;

const SETTINGS_STORE_FILE: &str = "settings";
pub const KEY_ANALYTICS_ENABLED: &str = "analytics_enabled";
pub const KEY_ANALYTICS_INSTALL_ID: &str = "analytics_install_id";
pub const KEY_PRIVACY_CONSENT_VERSION: &str = "privacy_consent_version";
pub const PRIVACY_CONSENT_VERSION: u64 = 1;
pub const ANALYTICS_DEFAULT_ENABLED: bool = true;

const POSTHOG_HOST: &str = "https://eu.i.posthog.com";
const INTERNAL_GENERATION_PROPERTY: &str = "_voicetypr_consent_generation";

#[cfg(debug_assertions)]
const POSTHOG_PROJECT_TOKEN: Option<&str> = None;
#[cfg(not(debug_assertions))]
const POSTHOG_PROJECT_TOKEN: Option<&str> = option_env!("POSTHOG_PROJECT_TOKEN");

#[derive(Debug)]
struct RuntimeState {
    enabled: bool,
    generation: u64,
    install_id: Option<String>,
}

static RUNTIME: LazyLock<RwLock<RuntimeState>> = LazyLock::new(|| {
    RwLock::new(RuntimeState {
        enabled: false,
        generation: 1,
        install_id: None,
    })
});
static CLIENT: OnceLock<Client> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAnalyticsConsent {
    pub enabled: bool,
    pub install_id: Option<String>,
    pub consent_required: bool,
}

impl StoredAnalyticsConsent {
    pub fn effective_enabled(&self) -> bool {
        self.enabled && !self.consent_required && self.install_id.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JourneyStage {
    Decode,
    Formatting,
    Delivery,
}

impl JourneyStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Decode => "decode",
            Self::Formatting => "formatting",
            Self::Delivery => "delivery",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JourneyOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

impl JourneyOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Whisper,
    Parakeet,
    Cloud,
    Remote,
}

impl EngineKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Whisper => "whisper",
            Self::Parakeet => "parakeet",
            Self::Cloud => "cloud",
            Self::Remote => "remote",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolishOutcome {
    Disabled,
    Skipped,
    Applied,
    Unchanged,
    Fallback,
}

impl PolishOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Skipped => "skipped",
            Self::Applied => "applied",
            Self::Unchanged => "unchanged",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolishPreset {
    PersonalDictation,
    CleanDictation,
    Writing,
    Notes,
    Message,
    Code,
}

impl PolishPreset {
    const fn as_str(self) -> &'static str {
        match self {
            Self::PersonalDictation => "personal_dictation",
            Self::CleanDictation => "clean_dictation",
            Self::Writing => "writing",
            Self::Notes => "notes",
            Self::Message => "message",
            Self::Code => "code",
        }
    }
}

impl From<crate::ai::prompts::EnhancementPreset> for PolishPreset {
    fn from(preset: crate::ai::prompts::EnhancementPreset) -> Self {
        match preset {
            crate::ai::prompts::EnhancementPreset::PersonalDictation => Self::PersonalDictation,
            crate::ai::prompts::EnhancementPreset::CleanDictation => Self::CleanDictation,
            crate::ai::prompts::EnhancementPreset::Writing => Self::Writing,
            crate::ai::prompts::EnhancementPreset::Notes => Self::Notes,
            crate::ai::prompts::EnhancementPreset::Message => Self::Message,
            crate::ai::prompts::EnhancementPreset::Code => Self::Code,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductEvent {
    AppStarted,
    OnboardingCompleted,
    RecordingStarted,
    RecordingStopped {
        duration_ms: Option<u64>,
    },
    StageFinished {
        stage: JourneyStage,
        outcome: JourneyOutcome,
        duration_ms: u64,
        engine: Option<EngineKind>,
    },
    PolishFinished {
        outcome: PolishOutcome,
        preset: PolishPreset,
        provider_id: String,
        model_id: String,
    },
}

impl ProductEvent {
    const fn name(&self) -> &'static str {
        match self {
            Self::AppStarted => "app.started",
            Self::OnboardingCompleted => "onboarding.completed",
            Self::RecordingStarted => "recording.started",
            Self::RecordingStopped { .. } => "recording.stopped",
            Self::StageFinished { .. } => "transcription.stage_finished",
            Self::PolishFinished { .. } => "polish.finished",
        }
    }
}

pub fn is_available() -> bool {
    POSTHOG_PROJECT_TOKEN.is_some_and(|token| !token.trim().is_empty())
}

#[cfg(test)]
fn is_enabled() -> bool {
    RUNTIME.read().enabled
}

pub fn read_consent(identifier: &str) -> StoredAnalyticsConsent {
    match settings_store_path(identifier) {
        Some(path) => read_consent_from_path(&path),
        None => StoredAnalyticsConsent {
            enabled: ANALYTICS_DEFAULT_ENABLED,
            install_id: None,
            consent_required: true,
        },
    }
}

fn settings_store_path(identifier: &str) -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(identifier).join(SETTINGS_STORE_FILE))
}

pub fn read_consent_from_path(path: &Path) -> StoredAnalyticsConsent {
    let value = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    consent_from_value(value.as_ref())
}

fn consent_from_value(value: Option<&Value>) -> StoredAnalyticsConsent {
    let version = value
        .and_then(|root| root.get(KEY_PRIVACY_CONSENT_VERSION))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let consent_required = version < PRIVACY_CONSENT_VERSION;
    let enabled = value
        .and_then(|root| root.get(KEY_ANALYTICS_ENABLED))
        .and_then(Value::as_bool)
        .unwrap_or(ANALYTICS_DEFAULT_ENABLED);
    let install_id = value
        .and_then(|root| root.get(KEY_ANALYTICS_INSTALL_ID))
        .and_then(Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .map(|value| value.to_string());

    StoredAnalyticsConsent {
        enabled,
        install_id,
        consent_required,
    }
}

/// Initializes the release-only PostHog client. The client exists while opted
/// out so a later opt-in is immediately effective, but every event still passes
/// through the generation-aware consent gate and strict allowlist.
pub fn init(consent: StoredAnalyticsConsent) {
    configure_runtime(
        consent.effective_enabled() && is_available(),
        consent.install_id,
    );

    let Some(token) = POSTHOG_PROJECT_TOKEN.filter(|token| !token.trim().is_empty()) else {
        return;
    };

    let mut options = ClientOptionsBuilder::default();
    options
        .api_key(token.to_string())
        .host(POSTHOG_HOST)
        .is_server(false)
        .request_timeout_seconds(5)
        // A serialized retry body cannot be rechecked after consent revocation.
        // One attempt leaves only an HTTP request that already started.
        .max_capture_attempts(1)
        .flush_at(20)
        .max_batch_size(20)
        .max_queue_size(256)
        .flush_interval_ms(5_000)
        .shutdown_timeout_ms(2_000)
        .before_send(scrub_event);
    let Ok(options) = options.build() else {
        log::warn!("Product analytics client configuration was rejected");
        return;
    };
    let _ = CLIENT.set(posthog_rs::client(options));

    capture(ProductEvent::AppStarted);
}

/// Enables analytics with a persisted anonymous installation id. Call only
/// after the store write succeeds.
pub fn enable(install_id: String) {
    let mut state = RUNTIME.write();
    state.install_id = Some(install_id);
    state.enabled = is_available();
}

/// Stops new egress, invalidates queued events, and forgets the in-memory id.
/// The store command deletes the persisted id separately.
pub fn disable() {
    let mut state = RUNTIME.write();
    state.enabled = false;
    state.generation = state.generation.wrapping_add(1);
    state.install_id = None;
}

fn configure_runtime(enabled: bool, install_id: Option<String>) {
    let mut state = RUNTIME.write();
    state.enabled = enabled && install_id.is_some();
    state.install_id = if state.enabled { install_id } else { None };
}

pub fn shutdown() {
    if let Some(client) = CLIENT.get() {
        client.shutdown();
    }
}

pub fn capture(event: ProductEvent) {
    let Some(client) = CLIENT.get() else {
        return;
    };
    let (install_id, generation) = {
        let state = RUNTIME.read();
        if !state.enabled {
            return;
        }
        let Some(install_id) = state.install_id.clone() else {
            return;
        };
        (install_id, state.generation)
    };

    let mut posthog_event = Event::new(event.name().to_string(), install_id);
    insert_base_properties(&mut posthog_event, generation);
    insert_event_properties(&mut posthog_event, event);
    client.capture(posthog_event);
}

fn insert_base_properties(event: &mut Event, generation: u64) {
    let _ = event.insert_prop("$process_person_profile", false);
    let _ = event.insert_prop("$geoip_disable", true);
    let _ = event.insert_prop("app_version", env!("CARGO_PKG_VERSION"));
    let _ = event.insert_prop("release_channel", RELEASE_CHANNEL);
    let _ = event.insert_prop("os", std::env::consts::OS);
    let _ = event.insert_prop("arch", std::env::consts::ARCH);
    let _ = event.insert_prop(INTERNAL_GENERATION_PROPERTY, generation);
}

fn insert_event_properties(event: &mut Event, product_event: ProductEvent) {
    match product_event {
        ProductEvent::AppStarted
        | ProductEvent::OnboardingCompleted
        | ProductEvent::RecordingStarted => {}
        ProductEvent::RecordingStopped { duration_ms } => {
            if let Some(duration_ms) = duration_ms {
                let _ = event.insert_prop("duration_bucket", duration_bucket(duration_ms));
            }
        }
        ProductEvent::StageFinished {
            stage,
            outcome,
            duration_ms,
            engine,
        } => {
            let _ = event.insert_prop("stage", stage.as_str());
            let _ = event.insert_prop("outcome", outcome.as_str());
            let _ = event.insert_prop("duration_bucket", duration_bucket(duration_ms));
            if let Some(engine) = engine {
                let _ = event.insert_prop("engine", engine.as_str());
            }
        }
        ProductEvent::PolishFinished {
            outcome,
            preset,
            provider_id,
            model_id,
        } => {
            let provider = safe_provider_id(&provider_id);
            let model = safe_model_id(&provider, &model_id);
            let attempted = matches!(
                outcome,
                PolishOutcome::Applied | PolishOutcome::Unchanged | PolishOutcome::Fallback
            );
            let _ = event.insert_prop("outcome", outcome.as_str());
            let _ = event.insert_prop("attempted", attempted);
            let _ = event.insert_prop("preset", preset.as_str());
            let _ = event.insert_prop("provider", provider);
            let _ = event.insert_prop("model", model);
        }
    }
}

fn duration_bucket(duration_ms: u64) -> &'static str {
    match duration_ms {
        0..=499 => "lt_500ms",
        500..=1_499 => "500_1499ms",
        1_500..=4_999 => "1500_4999ms",
        5_000..=14_999 => "5000_14999ms",
        15_000..=59_999 => "15000_59999ms",
        _ => "gte_60000ms",
    }
}

fn safe_provider_id(provider_id: &str) -> String {
    if matches!(provider_id, "none" | "unknown")
        || crate::ai::catalog::runtime_kind(provider_id).is_some()
    {
        provider_id.to_string()
    } else if provider_id.is_empty() {
        "none".to_string()
    } else {
        "unknown".to_string()
    }
}

fn safe_model_id(provider_id: &str, model_id: &str) -> String {
    if model_id.is_empty() {
        return "automatic".to_string();
    }
    if matches!(model_id, "automatic" | "custom")
        || crate::ai::catalog::all_provider_models(provider_id)
            .into_iter()
            .any(|model| model.model_id == model_id)
    {
        model_id.to_string()
    } else {
        "custom".to_string()
    }
}

fn scrub_event(mut event: Event) -> Option<Event> {
    let generation = event
        .properties()
        .get(INTERNAL_GENERATION_PROPERTY)
        .and_then(Value::as_u64)?;
    let state = RUNTIME.read();
    if !state.enabled || generation != state.generation {
        return None;
    }
    drop(state);
    event.remove_prop(INTERNAL_GENERATION_PROPERTY);

    let dynamic = validated_dynamic_properties(&event)?;
    let keys: Vec<String> = event.properties().keys().cloned().collect();
    for key in keys {
        event.remove_prop(&key);
    }

    let _ = event.insert_prop("$process_person_profile", false);
    let _ = event.insert_prop("$geoip_disable", true);
    let _ = event.insert_prop("app_version", env!("CARGO_PKG_VERSION"));
    let _ = event.insert_prop("release_channel", RELEASE_CHANNEL);
    let _ = event.insert_prop("os", std::env::consts::OS);
    let _ = event.insert_prop("arch", std::env::consts::ARCH);
    for (key, value) in dynamic {
        let _ = event.insert_prop(key, value);
    }
    Some(event)
}

fn validated_dynamic_properties(event: &Event) -> Option<Vec<(&'static str, Value)>> {
    let properties = event.properties();
    let string = |key: &str| properties.get(key).and_then(Value::as_str);
    let allowed = |value: &str, values: &[&str]| values.contains(&value);

    match event.event_name() {
        "app.started" | "onboarding.completed" | "recording.started" => Some(Vec::new()),
        "recording.stopped" => match string("duration_bucket") {
            Some(value) if allowed(value, DURATION_BUCKETS) => {
                Some(vec![("duration_bucket", Value::String(value.to_string()))])
            }
            None => Some(Vec::new()),
            _ => None,
        },
        "transcription.stage_finished" => {
            let stage = string("stage")?;
            let outcome = string("outcome")?;
            let duration = string("duration_bucket")?;
            if !allowed(stage, &["decode", "formatting", "delivery"])
                || !allowed(outcome, &["succeeded", "failed", "cancelled"])
                || !allowed(duration, DURATION_BUCKETS)
            {
                return None;
            }
            let mut safe = vec![
                ("stage", Value::String(stage.to_string())),
                ("outcome", Value::String(outcome.to_string())),
                ("duration_bucket", Value::String(duration.to_string())),
            ];
            if let Some(engine) = string("engine") {
                if !allowed(engine, &["whisper", "parakeet", "cloud", "remote"]) {
                    return None;
                }
                safe.push(("engine", Value::String(engine.to_string())));
            }
            Some(safe)
        }
        "polish.finished" => {
            let outcome = string("outcome")?;
            let preset = string("preset")?;
            let provider = string("provider")?;
            let model = string("model")?;
            let attempted = event.properties().get("attempted")?.as_bool()?;
            let expected_attempted = matches!(outcome, "applied" | "unchanged" | "fallback");
            if !allowed(
                outcome,
                &["disabled", "skipped", "applied", "unchanged", "fallback"],
            ) || !allowed(
                preset,
                &[
                    "personal_dictation",
                    "clean_dictation",
                    "writing",
                    "notes",
                    "message",
                    "code",
                ],
            ) || attempted != expected_attempted
                || safe_provider_id(provider) != provider
                || safe_model_id(provider, model) != model
            {
                return None;
            }
            Some(vec![
                ("outcome", Value::String(outcome.to_string())),
                ("attempted", Value::Bool(attempted)),
                ("preset", Value::String(preset.to_string())),
                ("provider", Value::String(provider.to_string())),
                ("model", Value::String(model.to_string())),
            ])
        }
        _ => None,
    }
}

const DURATION_BUCKETS: &[&str] = &[
    "lt_500ms",
    "500_1499ms",
    "1500_4999ms",
    "5000_14999ms",
    "15000_59999ms",
    "gte_60000ms",
];

#[cfg(test)]
mod tests {
    use super::*;

    static CONSENT_TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    fn event_with_generation(name: &str, generation: u64) -> Event {
        let mut event = Event::new(name.to_string(), "install-id".to_string());
        insert_base_properties(&mut event, generation);
        event
    }

    #[test]
    fn debug_build_is_inert_even_when_enabled() {
        let _guard = CONSENT_TEST_LOCK.lock();
        assert!(!is_available());
        enable("install-id".to_string());
        assert!(!is_enabled());
        disable();
    }

    #[test]
    fn missing_consent_version_requires_acknowledgement_and_blocks_egress() {
        let value = serde_json::json!({
            KEY_ANALYTICS_ENABLED: true,
            KEY_ANALYTICS_INSTALL_ID: "id",
        });
        let consent = consent_from_value(Some(&value));
        assert!(consent.consent_required);
        assert!(!consent.effective_enabled());
    }

    #[test]
    fn explicit_opt_out_remains_disabled_after_acknowledgement() {
        let value = serde_json::json!({
            KEY_PRIVACY_CONSENT_VERSION: PRIVACY_CONSENT_VERSION,
            KEY_ANALYTICS_ENABLED: false,
            KEY_ANALYTICS_INSTALL_ID: "id",
        });
        let consent = consent_from_value(Some(&value));
        assert!(!consent.consent_required);
        assert!(!consent.effective_enabled());
    }

    #[test]
    fn event_scrubber_rebuilds_properties_and_drops_unknown_fields() {
        let _guard = CONSENT_TEST_LOCK.lock();
        let generation = {
            let mut state = RUNTIME.write();
            state.enabled = true;
            state.generation
        };
        let mut event = event_with_generation("transcription.stage_finished", generation);
        event.insert_prop("stage", "decode").unwrap();
        event.insert_prop("outcome", "succeeded").unwrap();
        event.insert_prop("duration_bucket", "500_1499ms").unwrap();
        event.insert_prop("transcript", "must not leave").unwrap();

        let scrubbed = scrub_event(event).expect("known event accepted");
        assert_eq!(scrubbed.properties().get("transcript"), None);
        assert_eq!(
            scrubbed.properties().get("$process_person_profile"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            scrubbed.properties().get("$geoip_disable"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            scrubbed.properties().get("stage").and_then(Value::as_str),
            Some("decode")
        );
    }

    #[test]
    fn unknown_event_names_are_rejected() {
        let _guard = CONSENT_TEST_LOCK.lock();
        let generation = {
            let mut state = RUNTIME.write();
            state.enabled = true;
            state.generation
        };
        let event = event_with_generation("frontend.error", generation);
        assert!(scrub_event(event).is_none());
    }

    #[test]
    fn stale_generation_is_rejected_after_revocation() {
        let _guard = CONSENT_TEST_LOCK.lock();
        let generation = {
            let mut state = RUNTIME.write();
            state.enabled = true;
            state.generation
        };
        let event = event_with_generation("app.started", generation);
        RUNTIME.write().generation = generation.wrapping_add(1);
        assert!(scrub_event(event).is_none());
    }

    #[test]
    fn invalid_stored_install_id_is_never_used() {
        let value = serde_json::json!({
            KEY_PRIVACY_CONSENT_VERSION: PRIVACY_CONSENT_VERSION,
            KEY_ANALYTICS_ENABLED: true,
            KEY_ANALYTICS_INSTALL_ID: "/Users/example/private-transcript.txt",
        });
        let consent = consent_from_value(Some(&value));
        assert_eq!(consent.install_id, None);
        assert!(!consent.effective_enabled());
    }

    #[test]
    fn arbitrary_provider_and_model_values_are_bucketed() {
        assert_eq!(safe_provider_id("secret-provider"), "unknown");
        assert_eq!(safe_model_id("unknown", "private-model-name"), "custom");
    }

    #[test]
    fn polish_attempts_include_fallbacks_but_not_disabled_events() {
        for (outcome, expected) in [
            (PolishOutcome::Fallback, true),
            (PolishOutcome::Applied, true),
            (PolishOutcome::Unchanged, true),
            (PolishOutcome::Disabled, false),
            (PolishOutcome::Skipped, false),
        ] {
            let mut event = Event::new("polish.finished".to_string(), "install-id".to_string());
            insert_event_properties(
                &mut event,
                ProductEvent::PolishFinished {
                    outcome,
                    preset: PolishPreset::CleanDictation,
                    provider_id: "pi".to_string(),
                    model_id: String::new(),
                },
            );
            assert_eq!(
                event.properties().get("attempted").and_then(Value::as_bool),
                Some(expected)
            );
        }
    }

    #[test]
    fn duration_values_are_bounded_into_closed_buckets() {
        assert_eq!(duration_bucket(499), "lt_500ms");
        assert_eq!(duration_bucket(500), "500_1499ms");
        assert_eq!(duration_bucket(u64::MAX), "gte_60000ms");
    }
}
