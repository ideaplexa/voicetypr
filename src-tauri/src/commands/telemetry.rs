//! One persisted consent for PostHog analytics and coded errors.
use crate::{product_analytics, telemetry};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
static CONSENT_MUTATION_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
#[derive(Serialize)]
pub struct TelemetryStatus {
    pub enabled: bool,
    pub available: bool,
    pub consent_required: bool,
}
#[derive(Serialize)]
pub struct TelemetryConsentResult {
    pub enabled: bool,
    pub restart_required: bool,
}
#[tauri::command]
pub async fn get_telemetry_status(app: AppHandle) -> Result<TelemetryStatus, String> {
    let consent = product_analytics::read_consent(&app.config().identifier);
    Ok(TelemetryStatus {
        enabled: consent.enabled,
        available: telemetry::is_available(),
        consent_required: consent.consent_required,
    })
}
#[tauri::command]
pub async fn set_telemetry_consent(
    app: AppHandle,
    enabled: bool,
) -> Result<TelemetryConsentResult, String> {
    let _guard = CONSENT_MUTATION_LOCK.lock();
    if !enabled {
        telemetry::disable_and_drop_queued();
    }
    let store = app
        .store("settings")
        .map_err(|_| "Consent storage unavailable")?;
    let id = telemetry::save_consent_values(&store, enabled, true);
    store.save().map_err(|_| "Consent save failed")?;
    if let Some(id) = id {
        product_analytics::enable(id);
    }
    Ok(TelemetryConsentResult {
        enabled,
        restart_required: false,
    })
}
// Compatibility commands for already-open webviews; both mutate the same consent.
#[tauri::command]
pub async fn get_product_analytics_status(app: AppHandle) -> Result<TelemetryStatus, String> {
    get_telemetry_status(app).await
}
#[tauri::command]
pub async fn set_product_analytics_consent(
    app: AppHandle,
    enabled: bool,
) -> Result<TelemetryConsentResult, String> {
    set_telemetry_consent(app, enabled).await
}
#[tauri::command]
pub async fn defer_privacy_consent_for_session() -> Result<(), String> {
    product_analytics::disable();
    Ok(())
}
#[tauri::command]
pub async fn record_onboarding_completed() -> Result<(), String> {
    product_analytics::capture(product_analytics::ProductEvent::OnboardingCompleted);
    Ok(())
}
#[tauri::command]
pub async fn report_frontend_error(name: Option<String>, message: String) -> Result<(), String> {
    telemetry::capture_frontend_error(name.as_deref(), &message);
    Ok(())
}
#[tauri::command]
pub async fn record_observability_event(
    name: String,
    properties: serde_json::Value,
) -> Result<(), String> {
    if name == "island_peek_opened" {
        crate::observability::peek();
        return Ok(());
    }
    // The frontend cannot inject arbitrary event names, text, or error payloads.
    let schema = crate::observability::schema(&name).ok_or("Unknown event")?;
    let properties = schema
        .iter()
        .map(|(key, _)| {
            properties
                .get(*key)
                .cloned()
                .map(|v| (*key, v))
                .ok_or("Missing property")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let name = match name.as_str() {
        "insights_viewed" => "insights_viewed",
        "share_card_action" => "share_card_action",
        "settings_opened" => "settings_opened",
        "quick_setting_changed" => "quick_setting_changed",
        "island_action" => "island_action",
        _ => return Err("Native event only".into()),
    };
    let generation =
        (name == "island_action").then(crate::commands::audio::current_recording_generation);
    crate::observability::emit(name, properties, generation);
    Ok(())
}
#[derive(Serialize)]
pub struct ReportDiagnostics {
    app_version: &'static str,
    os: &'static str,
    arch: &'static str,
    install_id: Option<String>,
    dictation_ids: Vec<String>,
    engine: String,
    pill_mode: String,
}
#[tauri::command]
pub async fn get_report_diagnostics(app: AppHandle) -> Result<ReportDiagnostics, String> {
    let store = app
        .store("settings")
        .map_err(|_| "Diagnostics unavailable")?;
    let engine = store
        .get("current_model_engine")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let model = store
        .get("current_model")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let engine = if model.is_empty() {
        "none".into()
    } else {
        crate::pill::context::engine_short_name(&model, &engine)
    };
    let pill_mode = crate::commands::pill_feedback::pill_mode(&app);
    Ok(ReportDiagnostics {
        app_version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        install_id: product_analytics::install_id(),
        dictation_ids: crate::observability::last_ids(),
        engine,
        pill_mode,
    })
}
