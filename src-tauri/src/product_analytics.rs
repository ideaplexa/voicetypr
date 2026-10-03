//! Consent-gated analytics and Error Tracking through one PostHog EU client.
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use parking_lot::RwLock;

use posthog_rs::{ClientOptionsBuilder, Event};
use serde_json::Value;

use crate::release_channel::RELEASE_CHANNEL;

const SETTINGS_STORE_FILE: &str = "settings";
pub const KEY_ANALYTICS_ENABLED: &str = "analytics_enabled";
pub const KEY_ANALYTICS_INSTALL_ID: &str = "analytics_install_id";
pub const KEY_PRIVACY_CONSENT_VERSION: &str = "privacy_consent_version";
pub const PRIVACY_CONSENT_VERSION: u64 = 2;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationOutcome {
    Delivered,
    NoSpeech,
    Empty,
    Failed,
    Cancelled,
}

impl DictationOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::NoSpeech => "no_speech",
            Self::Empty => "empty",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationTransport {
    Local,
    Ws,
    Rest,
    RestFallback,
    Remote,
}

impl DictationTransport {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ws => "ws",
            Self::Rest => "rest",
            Self::RestFallback => "rest_fallback",
            Self::Remote => "remote",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationPaste {
    Succeeded,
    Failed,
    Skipped,
}

impl DictationPaste {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// Only typed, content-free facts may cross into the event builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationFacts {
    pub outcome: DictationOutcome,
    pub engine: EngineKind,
    pub model: String,
    pub transport: DictationTransport,
    pub live_preview: bool,
    pub recording_ms: u64,
    pub start_to_first_audio_ms: Option<u64>,
    pub stop_to_text_ms: u64,
    pub post_roll_speech_detected: bool,
    pub post_roll_interrupted: bool,
    pub words: usize,
    pub polish: PolishOutcome,
    pub paste: DictationPaste,
    pub app_category: crate::writing::AppCategory,
}

pub fn build_dictation_completed(facts: DictationFacts) -> ProductEvent {
    ProductEvent::DictationCompleted {
        outcome: facts.outcome,
        engine: facts.engine,
        model: safe_dictation_model(facts.engine, &facts.model),
        transport: facts.transport,
        live_preview: facts.live_preview,
        recording_ms: ((facts.recording_ms.min(600_000) + 5) / 10 * 10).min(600_000),
        start_to_first_audio_ms: facts.start_to_first_audio_ms.map(|ms| ms.min(10_000)),
        stop_to_text_ms: facts.stop_to_text_ms.min(600_000),
        post_roll_speech_detected: facts.post_roll_speech_detected,
        post_roll_interrupted: facts.post_roll_interrupted,
        words_bucket: words_bucket(facts.words),
        polish: facts.polish,
        paste: facts.paste,
        app_category: facts.app_category,
    }
}

fn words_bucket(words: usize) -> &'static str {
    match words {
        0 => "0",
        1..=5 => "1_5",
        6..=20 => "6_20",
        21..=60 => "21_60",
        61..=200 => "61_200",
        _ => "gt_200",
    }
}

fn safe_dictation_model(engine: EngineKind, model: &str) -> String {
    let known = match engine {
        EngineKind::Whisper => matches!(
            model,
            "base.en" | "small.en" | "large-v3" | "large-v3-turbo"
        ),
        EngineKind::Parakeet => crate::parakeet::models::AVAILABLE_MODELS
            .iter()
            .any(|entry| entry.id == model),
        EngineKind::Cloud => crate::cloud_stt::CloudProvider::ALL
            .iter()
            .any(|provider| provider.model_by_id(model).is_some()),
        EngineKind::Remote => false,
    };
    if known {
        model.to_string()
    } else {
        "other".to_string()
    }
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
    Observability {
        name: &'static str,
        properties: Vec<(&'static str, Value)>,
    },
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
    DictationCompleted {
        outcome: DictationOutcome,
        engine: EngineKind,
        model: String,
        transport: DictationTransport,
        live_preview: bool,
        recording_ms: u64,
        start_to_first_audio_ms: Option<u64>,
        stop_to_text_ms: u64,
        post_roll_speech_detected: bool,
        post_roll_interrupted: bool,
        words_bucket: &'static str,
        polish: PolishOutcome,
        paste: DictationPaste,
        app_category: crate::writing::AppCategory,
    },
}

impl ProductEvent {
    const fn name(&self) -> &'static str {
        match self {
            Self::AppStarted => "app_started",
            Self::Observability { name, .. } => name,
            Self::OnboardingCompleted => "onboarding.completed",
            Self::RecordingStarted => "recording.started",
            Self::RecordingStopped { .. } => "recording.stopped",
            Self::StageFinished { .. } => "transcription.stage_finished",
            Self::PolishFinished { .. } => "polish.finished",
            Self::DictationCompleted { .. } => "dictation_completed",
        }
    }
}

pub fn is_available() -> bool {
    POSTHOG_PROJECT_TOKEN.is_some_and(|token| !token.trim().is_empty())
}

pub fn is_enabled() -> bool {
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

pub(crate) fn consent_from_value(value: Option<&Value>) -> StoredAnalyticsConsent {
    let version = value
        .and_then(|root| root.get(KEY_PRIVACY_CONSENT_VERSION))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let consent_required = version < PRIVACY_CONSENT_VERSION;
    let enabled = crate::telemetry::migrated_consent(value);
    let install_id = value
        .and_then(|root| {
            root.get(KEY_ANALYTICS_INSTALL_ID)
                .or_else(|| root.get(crate::telemetry::KEY_TELEMETRY_INSTALL_ID))
        })
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
        .error_tracking(
            posthog_rs::ErrorTrackingOptionsBuilder::default()
                .capture_panics(true)
                .in_app_exclude_paths(vec!["/.cargo/".into(), "/rustc/".into(), "registry".into()])
                .build()
                .expect("static error tracking options"),
        )
        .before_send(scrub_event);
    let Ok(options) = options.build() else {
        log::warn!("Product analytics client configuration was rejected");
        return;
    };
    // Default Rust panic printing can include arbitrary payload text. Preserve
    // only a fixed local notice, then install SDK capture plus our coded bridge.
    std::panic::set_hook(Box::new(|_| eprintln!("Application panic")));
    if posthog_rs::init_global(options).is_ok() {
        crate::telemetry::install_coded_panic_hook();
    }
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
    revoke_with_flush(posthog_rs::flush);
}

fn revoke_with_flush(flush: impl FnOnce()) {
    let mut state = RUNTIME.write();
    state.enabled = false;
    state.generation = state.generation.wrapping_add(1);
    state.install_id = None;
    drop(state);
    crate::observability::clear_peeks();
    // The SDK has no pre-HTTP hook. Drain synchronously while the gate is off:
    // buffered events are rejected by before_send; a batch already prepared
    // by the worker can finish its single attempt (bounded by request timeout).
    flush();
}

pub(crate) fn consent_epoch() -> Option<u64> {
    let state = RUNTIME.read();
    state.enabled.then_some(state.generation)
}

fn configure_runtime(enabled: bool, install_id: Option<String>) {
    let mut state = RUNTIME.write();
    state.enabled = enabled && install_id.is_some();
    state.install_id = if state.enabled { install_id } else { None };
}

pub fn shutdown() {
    crate::observability::flush_peeks();
    posthog_rs::shutdown();
}

pub fn capture(event: ProductEvent) {
    capture_at(
        event,
        crate::commands::audio::current_recording_generation(),
    );
}
pub fn capture_at(event: ProductEvent, dictation_generation: u64) {
    capture_at_epoch(event, dictation_generation, None);
}

pub(crate) fn capture_at_epoch(event: ProductEvent, dictation_generation: u64, epoch: Option<u64>) {
    capture_with_trace(event, dictation_generation, epoch, None);
}

pub(crate) fn capture_with_trace(
    event: ProductEvent,
    dictation_generation: u64,
    epoch: Option<u64>,
    trace_id: Option<String>,
) {
    #[cfg(test)]
    TEST_CAPTURES.with(|captures| {
        captures.borrow_mut().push((
            event.name().to_string(),
            dictation_generation,
            trace_id
                .clone()
                .or_else(|| crate::observability::trace_id(dictation_generation)),
        ))
    });
    let (install_id, generation) = {
        let state = RUNTIME.read();
        if !state.enabled || epoch.is_some_and(|epoch| epoch != state.generation) {
            return;
        }
        let Some(install_id) = state.install_id.clone() else {
            return;
        };
        (install_id, state.generation)
    };

    let mut posthog_event = Event::new(event.name().to_string(), install_id);
    insert_base_properties(&mut posthog_event, generation);
    let completed = matches!(&event, ProductEvent::DictationCompleted { .. });
    insert_event_properties(&mut posthog_event, event);
    if completed {
        for (key, value) in crate::observability::completion_properties(dictation_generation) {
            let _ = posthog_event.insert_prop(key, value);
        }
    }
    if let Some(id) = trace_id.or_else(|| crate::observability::trace_id(dictation_generation)) {
        let _ = posthog_event.insert_prop("dictation_id", id);
    }
    posthog_rs::capture(posthog_event);
}

#[cfg(test)]
thread_local! {
    static TEST_CAPTURES: std::cell::RefCell<Vec<(String, u64, Option<String>)>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
pub(crate) fn take_test_captures() -> Vec<(String, u64, Option<String>)> {
    TEST_CAPTURES.with(|captures| std::mem::take(&mut *captures.borrow_mut()))
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
        ProductEvent::Observability { properties, .. } => {
            for (key, value) in properties {
                let _ = event.insert_prop(key, value);
            }
        }
        ProductEvent::AppStarted => {
            for (key, value) in crate::observability::startup_properties() {
                let _ = event.insert_prop(key, value);
            }
        }
        ProductEvent::OnboardingCompleted | ProductEvent::RecordingStarted => {}
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
        ProductEvent::DictationCompleted {
            outcome,
            engine,
            model,
            transport,
            live_preview,
            recording_ms,
            start_to_first_audio_ms,
            stop_to_text_ms,
            post_roll_speech_detected,
            post_roll_interrupted,
            words_bucket,
            polish,
            paste,
            app_category,
        } => {
            let _ = event.insert_prop("outcome", outcome.as_str());
            let _ = event.insert_prop("engine", engine.as_str());
            let _ = event.insert_prop("model", model);
            let _ = event.insert_prop("transport", transport.as_str());
            let _ = event.insert_prop("live_preview", live_preview);
            let _ = event.insert_prop("recording_ms", recording_ms);
            if let Some(value) = start_to_first_audio_ms {
                let _ = event.insert_prop("start_to_first_audio_ms", value);
            }
            let _ = event.insert_prop("stop_to_text_ms", stop_to_text_ms);
            let _ = event.insert_prop("post_roll_speech_detected", post_roll_speech_detected);
            let _ = event.insert_prop("post_roll_interrupted", post_roll_interrupted);
            let _ = event.insert_prop("words_bucket", words_bucket);
            let _ = event.insert_prop("polish", polish.as_str());
            let _ = event.insert_prop("paste", paste.as_str());
            let _ = event.insert_prop("app_category", app_category.analytics_label());
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

const SDK_ENRICHMENT_KEYS: &[&str] = &[
    "$os",
    "$os_version",
    "$lib",
    "$lib_version",
    "$lib_version__major",
    "$lib_version__minor",
    "$lib_version__patch",
];

fn scrub_event(mut event: Event) -> Option<Event> {
    // Exactly the enrichment applied by pinned posthog-rs 0.22 V0 pipeline.
    // Never accept an arbitrary dollar-prefixed property.
    for key in SDK_ENRICHMENT_KEYS {
        event.remove_prop(key);
    }
    if event.event_name() == "$exception" {
        return crate::telemetry::scrub_exception_event(event);
    }
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

    let trace_id = event
        .properties()
        .get("dictation_id")
        .and_then(Value::as_str)
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .filter(|id| id.get_version_num() == 4);
    event.remove_prop("dictation_id");
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
    if let Some(id) = trace_id {
        let _ = event.insert_prop("dictation_id", id.to_string());
    }
    for (key, value) in dynamic {
        let _ = event.insert_prop(key, value);
    }
    Some(event)
}

fn validated_dynamic_properties(event: &Event) -> Option<Vec<(&'static str, Value)>> {
    if crate::observability::schema(event.event_name()).is_some() {
        return crate::observability::validated(event);
    }
    let properties = event.properties();
    let string = |key: &str| properties.get(key).and_then(Value::as_str);
    let allowed = |value: &str, values: &[&str]| values.contains(&value);

    match event.event_name() {
        "app_started" | "onboarding.completed" | "recording.started" => Some(Vec::new()),
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
        "dictation_completed" => {
            const DYNAMIC_KEYS: &[&str] = &[
                "start_source",
                "mode",
                "start_card_shown",
                "island_start_details",
                "language",
                "polish_skip_used",
                "polish_guard_reason",
                "polish_keep_words",
                "polish_style",
                "recovery_kind",
                "recovery_resolution",
                "focus_safe",
                "outcome",
                "engine",
                "model",
                "transport",
                "live_preview",
                "recording_ms",
                "start_to_first_audio_ms",
                "stop_to_text_ms",
                "post_roll_speech_detected",
                "post_roll_interrupted",
                "words_bucket",
                "polish",
                "paste",
                "app_category",
            ];
            const BASE_KEYS: &[&str] = &[
                "$process_person_profile",
                "$geoip_disable",
                "app_version",
                "release_channel",
                "os",
                "arch",
                INTERNAL_GENERATION_PROPERTY,
            ];
            if properties.keys().any(|key| {
                !DYNAMIC_KEYS.contains(&key.as_str()) && !BASE_KEYS.contains(&key.as_str())
            }) {
                return None;
            }
            let outcome = string("outcome")?;
            let engine = string("engine")?;
            let model = string("model")?;
            let transport = string("transport")?;
            let words_bucket = string("words_bucket")?;
            let polish = string("polish")?;
            let paste = string("paste")?;
            let app_category = string("app_category")?;
            let engine_kind = match engine {
                "whisper" => EngineKind::Whisper,
                "parakeet" => EngineKind::Parakeet,
                "cloud" => EngineKind::Cloud,
                "remote" => EngineKind::Remote,
                _ => return None,
            };
            if !allowed(
                outcome,
                &["delivered", "no_speech", "empty", "failed", "cancelled"],
            ) || safe_dictation_model(engine_kind, model) != model
                || !allowed(
                    transport,
                    &["local", "ws", "rest", "rest_fallback", "remote"],
                )
                || !allowed(
                    words_bucket,
                    &["0", "1_5", "6_20", "21_60", "61_200", "gt_200"],
                )
                || !allowed(
                    polish,
                    &["disabled", "skipped", "applied", "unchanged", "fallback"],
                )
                || !allowed(paste, &["succeeded", "failed", "skipped"])
                || !allowed(
                    app_category,
                    &[
                        "chat", "email", "docs", "code", "terminal", "social", "notes", "browser",
                        "other",
                    ],
                )
            {
                return None;
            }
            let boolean = |key: &str| properties.get(key)?.as_bool();
            let integer =
                |key: &str, max: u64| properties.get(key)?.as_u64().filter(|value| *value <= max);
            let live_preview = boolean("live_preview")?;
            let recording_ms = integer("recording_ms", 600_000)?;
            if recording_ms % 10 != 0 {
                return None;
            }
            let stop_to_text_ms = integer("stop_to_text_ms", 600_000)?;
            let speech_detected = boolean("post_roll_speech_detected")?;
            let interrupted = boolean("post_roll_interrupted")?;
            let mut safe = vec![
                ("outcome", Value::String(outcome.to_string())),
                ("engine", Value::String(engine.to_string())),
                ("model", Value::String(model.to_string())),
                ("transport", Value::String(transport.to_string())),
                ("live_preview", Value::Bool(live_preview)),
                ("recording_ms", Value::from(recording_ms)),
                ("stop_to_text_ms", Value::from(stop_to_text_ms)),
                ("post_roll_speech_detected", Value::Bool(speech_detected)),
                ("post_roll_interrupted", Value::Bool(interrupted)),
                ("words_bucket", Value::String(words_bucket.to_string())),
                ("polish", Value::String(polish.to_string())),
                ("paste", Value::String(paste.to_string())),
                ("app_category", Value::String(app_category.to_string())),
            ];
            if properties.contains_key("start_to_first_audio_ms") {
                safe.push((
                    "start_to_first_audio_ms",
                    Value::from(integer("start_to_first_audio_ms", 10_000)?),
                ));
            }
            for (key, vocabulary) in crate::observability::COMPLETION_SCHEMA {
                if let Some(value) = properties.get(*key) {
                    if vocabulary.is_empty() {
                        value.as_bool()?;
                    } else if !vocabulary.contains(&value.as_str()?) {
                        return None;
                    }
                    safe.push((*key, value.clone()));
                }
            }
            Some(safe)
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

pub(crate) fn exception_options(
    code: &str,
    generation: Option<u64>,
) -> Option<posthog_rs::CaptureExceptionOptions> {
    let state = RUNTIME.read();
    if !state.enabled {
        return None;
    }
    let mut options = posthog_rs::CaptureExceptionOptions::new()
        .distinct_id(state.install_id.clone()?)
        .fingerprint(code)
        .level("error")
        .property("code", code)
        .ok()?
        .property("_voicetypr_consent_generation", state.generation)
        .ok()?;
    if let Some(id) = generation.and_then(crate::observability::trace_id) {
        options = options.property("dictation_id", id).ok()?;
    }
    Some(options)
}
pub(crate) fn exception_allowed(event: &Event) -> bool {
    let state = RUNTIME.read();
    state.enabled
        && event
            .properties()
            .get("_voicetypr_consent_generation")
            .and_then(Value::as_u64)
            .is_some_and(|generation| generation == state.generation)
}
pub fn install_id() -> Option<String> {
    let state = RUNTIME.read();
    if state.enabled {
        state.install_id.clone()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictation_facts() -> DictationFacts {
        DictationFacts {
            outcome: DictationOutcome::Delivered,
            engine: EngineKind::Cloud,
            model: "stt-async-v5".to_string(),
            transport: DictationTransport::Ws,
            live_preview: true,
            recording_ms: 1_234,
            start_to_first_audio_ms: Some(120),
            stop_to_text_ms: 830,
            post_roll_speech_detected: false,
            post_roll_interrupted: false,
            words: 12,
            polish: PolishOutcome::Applied,
            paste: DictationPaste::Succeeded,
            app_category: crate::writing::AppCategory::Chat,
        }
    }

    fn dictation_event() -> Event {
        let mut event = Event::new("dictation_completed".to_string(), "install-id".to_string());
        insert_event_properties(&mut event, build_dictation_completed(dictation_facts()));
        event
    }

    #[test]
    #[serial_test::serial(analytics_runtime)]
    fn dictation_completed_exact_property_set_and_rounding() {
        let event = dictation_event();
        let mut keys: Vec<&str> = event.properties().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "app_category",
                "engine",
                "live_preview",
                "model",
                "outcome",
                "paste",
                "polish",
                "post_roll_interrupted",
                "post_roll_speech_detected",
                "recording_ms",
                "start_to_first_audio_ms",
                "stop_to_text_ms",
                "transport",
                "words_bucket",
            ]
        );
        assert_eq!(event.properties()["recording_ms"], Value::from(1_230));
        assert_eq!(event.properties()["words_bucket"], Value::from("6_20"));
        assert!(validated_dynamic_properties(&event).is_some());
        let mut no_first_audio = dictation_facts();
        no_first_audio.start_to_first_audio_ms = None;
        no_first_audio.recording_ms = u64::MAX;
        no_first_audio.stop_to_text_ms = u64::MAX;
        let mut event = Event::new("dictation_completed".to_string(), "install-id".to_string());
        insert_event_properties(&mut event, build_dictation_completed(no_first_audio));
        assert!(!event.properties().contains_key("start_to_first_audio_ms"));
        assert_eq!(event.properties()["recording_ms"], Value::from(600_000));
        assert_eq!(event.properties()["stop_to_text_ms"], Value::from(600_000));
    }

    #[test]
    #[serial_test::serial(analytics_runtime)]
    fn dictation_completed_builder_covers_outcomes_and_privacy() {
        for outcome in [
            DictationOutcome::Delivered,
            DictationOutcome::NoSpeech,
            DictationOutcome::Empty,
            DictationOutcome::Failed,
            DictationOutcome::Cancelled,
        ] {
            let mut facts = dictation_facts();
            facts.outcome = outcome;
            facts.model = "private/custom/model".to_string();
            let mut event = Event::new("dictation_completed".to_string(), "install-id".to_string());
            insert_event_properties(&mut event, build_dictation_completed(facts));
            assert_eq!(event.properties()["outcome"], Value::from(outcome.as_str()));
            assert_eq!(event.properties()["model"], Value::from("other"));
            assert!(validated_dynamic_properties(&event).is_some());
        }
        assert_eq!(words_bucket(0), "0");
        assert_eq!(words_bucket(5), "1_5");
        assert_eq!(words_bucket(20), "6_20");
        assert_eq!(words_bucket(60), "21_60");
        assert_eq!(words_bucket(200), "61_200");
        assert_eq!(words_bucket(201), "gt_200");
    }

    #[test]
    #[serial_test::serial(analytics_runtime)]
    fn dictation_completed_validator_drops_bad_values_and_extra_keys() {
        for (key, wrong_type) in [
            ("outcome", serde_json::json!(true)),
            ("engine", serde_json::json!(true)),
            ("model", serde_json::json!(true)),
            ("transport", serde_json::json!(true)),
            ("live_preview", serde_json::json!("true")),
            ("recording_ms", serde_json::json!("1230")),
            ("start_to_first_audio_ms", serde_json::json!("120")),
            ("stop_to_text_ms", serde_json::json!("830")),
            ("post_roll_speech_detected", serde_json::json!("false")),
            ("post_roll_interrupted", serde_json::json!("false")),
            ("words_bucket", serde_json::json!(true)),
            ("polish", serde_json::json!(true)),
            ("paste", serde_json::json!(true)),
            ("app_category", serde_json::json!(true)),
        ] {
            let mut missing = dictation_event();
            missing.remove_prop(key);
            if key == "start_to_first_audio_ms" {
                assert!(validated_dynamic_properties(&missing).is_some());
            } else {
                assert!(
                    validated_dynamic_properties(&missing).is_none(),
                    "accepted missing {key}"
                );
            }
            for value in [serde_json::Value::Null, wrong_type] {
                let mut event = dictation_event();
                event.insert_prop(key, value).unwrap();
                assert!(
                    validated_dynamic_properties(&event).is_none(),
                    "accepted invalid {key}"
                );
            }
        }
        let bad_values = [
            ("outcome", serde_json::json!("private")),
            ("engine", serde_json::json!("private")),
            ("model", serde_json::json!("private")),
            ("transport", serde_json::json!("private")),
            ("live_preview", serde_json::json!("true")),
            ("recording_ms", serde_json::json!(600_001)),
            ("start_to_first_audio_ms", serde_json::json!(10_001)),
            ("stop_to_text_ms", serde_json::json!(600_001)),
            ("post_roll_speech_detected", serde_json::json!(1)),
            ("post_roll_interrupted", serde_json::json!(1)),
            ("words_bucket", serde_json::json!("private")),
            ("polish", serde_json::json!("private")),
            ("paste", serde_json::json!("private")),
            ("app_category", serde_json::json!("private")),
        ];
        for (key, value) in bad_values {
            let mut event = dictation_event();
            event.insert_prop(key, value).unwrap();
            assert!(
                validated_dynamic_properties(&event).is_none(),
                "accepted {key}"
            );
        }
        for (key, value) in [
            ("recording_ms", serde_json::json!(-1)),
            ("recording_ms", serde_json::json!(1.5)),
            ("recording_ms", serde_json::json!(11)),
            ("start_to_first_audio_ms", serde_json::json!(-1)),
            ("stop_to_text_ms", serde_json::json!(1.5)),
        ] {
            let mut event = dictation_event();
            event.insert_prop(key, value).unwrap();
            assert!(
                validated_dynamic_properties(&event).is_none(),
                "accepted {key}"
            );
        }
        let mut event = dictation_event();
        event.insert_prop("transcript", "private").unwrap();
        assert!(validated_dynamic_properties(&event).is_none());
        let mut event = dictation_event();
        event.remove_prop("outcome");
        assert!(validated_dynamic_properties(&event).is_none());
    }

    #[test]
    #[serial_test::serial(analytics_runtime)]
    fn dictation_completed_scrubber_keeps_only_closed_properties() {
        let _guard = CONSENT_TEST_LOCK.lock();
        let generation = {
            let mut state = RUNTIME.write();
            state.enabled = true;
            state.generation
        };
        let mut event = event_with_generation("dictation_completed", generation);
        insert_event_properties(&mut event, build_dictation_completed(dictation_facts()));
        let scrubbed = scrub_event(event).expect("closed dictation event accepted");
        assert_eq!(scrubbed.properties()["outcome"], Value::from("delivered"));
        assert!(!scrubbed
            .properties()
            .contains_key(INTERNAL_GENERATION_PROPERTY));
    }

    #[tokio::test]
    #[serial_test::serial(analytics_runtime)]
    async fn sdk_batch_enrichment_and_revocation_use_real_worker() {
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let host = server.uri();
        let enriched = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = enriched.clone();
        tokio::task::spawn_blocking(move || {
            configure_runtime(true, Some(uuid::Uuid::new_v4().to_string()));
            let epoch = consent_epoch().unwrap();
            let mut options = ClientOptionsBuilder::default();
            options
                .api_key("test-token".to_string())
                .host(host)
                .is_server(false)
                .flush_at(100usize)
                .max_batch_size(20usize)
                .flush_interval_ms(60_000u64)
                .max_capture_attempts(1u32)
                .before_send(move |event: Event| {
                    for key in SDK_ENRICHMENT_KEYS {
                        assert!(
                            event.properties().contains_key(*key),
                            "missing enrichment {key}"
                        );
                    }
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Some(event)
                })
                .before_send(scrub_event);
            let client = posthog_rs::client(options.build().unwrap());
            let mut completed = event_with_generation("dictation_completed", epoch);
            insert_event_properties(&mut completed, build_dictation_completed(dictation_facts()));
            client.capture(completed);
            client.capture(event_with_generation("recording.started", epoch));
            client.capture(event_with_generation("onboarding.completed", epoch));
            client.flush();
            // Accepted in the worker queue, then revoked before dispatch.
            client.capture(event_with_generation("recording.started", epoch));
            let mut exception = event_with_generation("$exception", epoch);
            exception.insert_prop("code", "retry_failed").unwrap();
            client.capture(exception);
            revoke_with_flush(|| client.flush());
            assert!(consent_epoch().is_none());
            // A capture that raced revoke/enqueue cannot revive after opt-in.
            configure_runtime(true, Some(uuid::Uuid::new_v4().to_string()));
            client.capture(event_with_generation("recording.started", epoch));
            client.flush();
            client.shutdown();
            configure_runtime(false, None);
        })
        .await
        .unwrap();
        assert_eq!(enriched.load(std::sync::atomic::Ordering::SeqCst), 6);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "revoked events or exceptions escaped");
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        let batch = body["batch"].as_array().unwrap();
        assert_eq!(batch.len(), 3);
        for (event, name) in batch.iter().zip([
            "dictation_completed",
            "recording.started",
            "onboarding.completed",
        ]) {
            assert_eq!(event["event"], name);
            for key in SDK_ENRICHMENT_KEYS {
                assert!(event["properties"].get(*key).is_none());
            }
            assert!(event["properties"]
                .get(INTERNAL_GENERATION_PROPERTY)
                .is_none());
        }
        assert_eq!(batch[0]["properties"]["outcome"], "delivered");
    }

    #[tokio::test]
    #[serial_test::serial(analytics_runtime)]
    async fn revocation_waits_for_prepared_batch_and_drops_the_remaining_queue() {
        use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let host = server.uri();
        tokio::task::spawn_blocking(move || {
            configure_runtime(true, Some(uuid::Uuid::new_v4().to_string()));
            let epoch = consent_epoch().unwrap();
            let (entered_tx, entered_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let release_rx = std::sync::Mutex::new(release_rx);
            let mut options = ClientOptionsBuilder::default();
            options
                .api_key("test-token".to_string())
                .host(host)
                .is_server(false)
                .flush_at(1usize)
                .max_capture_attempts(1u32)
                .before_send(scrub_event)
                .before_send(move |event: Event| {
                    // Pause after the final consent check: this is the SDK's
                    // unavoidable prepared-batch window, before HTTP starts.
                    entered_tx.send(()).unwrap();
                    release_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(5))
                        .unwrap();
                    Some(event)
                });
            let client = std::sync::Arc::new(posthog_rs::client(options.build().unwrap()));
            client.capture(event_with_generation("recording.started", epoch));
            entered_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            let revoking_client = client.clone();
            let (disabled_tx, disabled_rx) = std::sync::mpsc::channel();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let revocation = std::thread::spawn(move || {
                revoke_with_flush(|| {
                    disabled_tx.send(()).unwrap();
                    revoking_client.flush();
                });
                done_tx.send(()).unwrap();
            });
            disabled_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            assert!(
                done_rx
                    .recv_timeout(std::time::Duration::from_millis(10))
                    .is_err(),
                "revocation returned before the accepted batch drained"
            );
            client.capture(event_with_generation("recording.started", epoch));
            let mut exception = event_with_generation("$exception", epoch);
            exception.insert_prop("code", "retry_failed").unwrap();
            client.capture(exception);
            release_tx.send(()).unwrap();
            revocation.join().unwrap();
            client.shutdown();
            configure_runtime(false, None);
        })
        .await
        .unwrap();
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "pending events or retries escaped");
        let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["batch"].as_array().unwrap().len(), 1);
    }

    static CONSENT_TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

    fn event_with_generation(name: &str, generation: u64) -> Event {
        let mut event = Event::new(name.to_string(), "install-id".to_string());
        insert_base_properties(&mut event, generation);
        event
    }

    #[test]
    #[serial_test::serial(analytics_runtime)]
    fn debug_build_is_inert_even_when_enabled() {
        let _guard = CONSENT_TEST_LOCK.lock();
        assert!(!is_available());
        enable("install-id".to_string());
        assert!(!is_enabled());
        disable();
    }

    #[test]
    #[serial_test::serial(analytics_runtime)]
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
    #[serial_test::serial(analytics_runtime)]
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
    #[serial_test::serial(analytics_runtime)]
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
    #[serial_test::serial(analytics_runtime)]
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
    #[serial_test::serial(analytics_runtime)]
    fn stale_generation_is_rejected_after_revocation() {
        let _guard = CONSENT_TEST_LOCK.lock();
        let generation = {
            let mut state = RUNTIME.write();
            state.enabled = true;
            state.generation
        };
        let event = event_with_generation("app_started", generation);
        RUNTIME.write().generation = generation.wrapping_add(1);
        assert!(scrub_event(event).is_none());
    }

    #[test]
    #[serial_test::serial(analytics_runtime)]
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
    #[serial_test::serial(analytics_runtime)]
    fn arbitrary_provider_and_model_values_are_bucketed() {
        assert_eq!(safe_provider_id("secret-provider"), "unknown");
        assert_eq!(safe_model_id("unknown", "private-model-name"), "custom");
    }

    #[test]
    #[serial_test::serial(analytics_runtime)]
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
    #[serial_test::serial(analytics_runtime)]
    fn duration_values_are_bounded_into_closed_buckets() {
        assert_eq!(duration_bucket(499), "lt_500ms");
        assert_eq!(duration_bucket(500), "500_1499ms");
        assert_eq!(duration_bucket(u64::MAX), "gte_60000ms");
    }
}
