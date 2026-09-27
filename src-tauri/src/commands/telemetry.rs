//! Tauri commands for independent GlitchTip diagnostics and PostHog product
//! analytics consent. Each category owns dedicated store keys; generic settings
//! saves never mutate consent.

use crate::{product_analytics, telemetry};
use serde::Serialize;
use serde_json::json;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const SETTINGS_STORE_FILE: &str = "settings";

static CONSENT_MUTATION_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

#[derive(Serialize)]
pub struct TelemetryStatus {
    /// Whether reporting is enabled (on by default unless opted out).
    pub enabled: bool,
    /// Whether this build can report at all (a DSN was compiled in).
    pub available: bool,
}

#[derive(Serialize)]
pub struct TelemetryConsentResult {
    pub enabled: bool,
    /// Enabling mid-session needs a restart to actually wire the Sentry client
    /// (it is only initialized at startup). Disabling takes effect immediately.
    pub restart_required: bool,
}

#[derive(Serialize)]
pub struct ProductAnalyticsStatus {
    /// The stored preference. Egress remains blocked while consent is required.
    pub enabled: bool,
    pub available: bool,
    pub consent_required: bool,
}

#[derive(Serialize)]
pub struct ProductAnalyticsConsentResult {
    pub enabled: bool,
}

/// Returns the current consent + whether reporting is possible in this build.
#[tauri::command]
pub async fn get_telemetry_status(app: AppHandle) -> Result<TelemetryStatus, String> {
    let store = app.store(SETTINGS_STORE_FILE).map_err(|e| e.to_string())?;
    let enabled = store
        .get(telemetry::KEY_TELEMETRY_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(telemetry::TELEMETRY_DEFAULT_ENABLED);
    Ok(TelemetryStatus {
        enabled,
        available: telemetry::is_available(),
    })
}

/// Persists the consent choice. On opt-in, mints an anonymous install id if one
/// does not exist; on opt-out, deletes it (a fresh id is minted if re-enabled).
#[tauri::command]
pub async fn set_telemetry_consent(
    app: AppHandle,
    enabled: bool,
) -> Result<TelemetryConsentResult, String> {
    let _guard = CONSENT_MUTATION_LOCK.lock();
    // Close the gate and discard queued envelopes before touching persistence.
    // A request already handed to the HTTP stack may still complete.
    if !enabled {
        telemetry::disable_and_drop_queued();
    }

    let store = app.store(SETTINGS_STORE_FILE).map_err(|e| e.to_string())?;
    let was_enabled = store
        .get(telemetry::KEY_TELEMETRY_ENABLED)
        .and_then(|v| v.as_bool())
        .unwrap_or(telemetry::TELEMETRY_DEFAULT_ENABLED);

    store.set(telemetry::KEY_TELEMETRY_ENABLED, json!(enabled));
    if enabled {
        let has_id = store
            .get(telemetry::KEY_TELEMETRY_INSTALL_ID)
            .and_then(|value| {
                value
                    .as_str()
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
            })
            .is_some();
        if !has_id {
            store.set(
                telemetry::KEY_TELEMETRY_INSTALL_ID,
                json!(uuid::Uuid::new_v4().to_string()),
            );
        }
    } else {
        store.delete(telemetry::KEY_TELEMETRY_INSTALL_ID);
    }
    store.save().map_err(|e| e.to_string())?;

    // Only turn the gate ON after a successful persist; enabling is fully
    // effective next launch (the client/panic hook are wired at startup).
    if enabled {
        telemetry::set_enabled(true);
    }
    let restart_required = enabled && !was_enabled;

    Ok(TelemetryConsentResult {
        enabled,
        restart_required,
    })
}

#[tauri::command]
pub async fn get_product_analytics_status(
    app: AppHandle,
) -> Result<ProductAnalyticsStatus, String> {
    let store = app.store(SETTINGS_STORE_FILE).map_err(|e| e.to_string())?;
    let enabled = store
        .get(product_analytics::KEY_ANALYTICS_ENABLED)
        .and_then(|value| value.as_bool())
        .unwrap_or(product_analytics::ANALYTICS_DEFAULT_ENABLED);
    let consent_version = store
        .get(product_analytics::KEY_PRIVACY_CONSENT_VERSION)
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    Ok(ProductAnalyticsStatus {
        enabled,
        available: product_analytics::is_available(),
        consent_required: consent_version < product_analytics::PRIVACY_CONSENT_VERSION,
    })
}

#[tauri::command]
pub async fn set_product_analytics_consent(
    app: AppHandle,
    enabled: bool,
) -> Result<ProductAnalyticsConsentResult, String> {
    let _guard = CONSENT_MUTATION_LOCK.lock();
    if !enabled {
        product_analytics::disable();
    }

    let store = app.store(SETTINGS_STORE_FILE).map_err(|e| e.to_string())?;
    store.set(
        product_analytics::KEY_PRIVACY_CONSENT_VERSION,
        json!(product_analytics::PRIVACY_CONSENT_VERSION),
    );
    store.set(product_analytics::KEY_ANALYTICS_ENABLED, json!(enabled));

    let install_id = if enabled {
        let existing = store
            .get(product_analytics::KEY_ANALYTICS_INSTALL_ID)
            .and_then(|value| {
                value
                    .as_str()
                    .and_then(|value| uuid::Uuid::parse_str(value).ok())
            })
            .map(|value| value.to_string());
        let install_id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        store.set(
            product_analytics::KEY_ANALYTICS_INSTALL_ID,
            json!(install_id),
        );
        Some(install_id)
    } else {
        store.delete(product_analytics::KEY_ANALYTICS_INSTALL_ID);
        None
    };
    store.save().map_err(|e| e.to_string())?;

    if let Some(install_id) = install_id {
        product_analytics::enable(install_id);
    }

    Ok(ProductAnalyticsConsentResult { enabled })
}

/// Keeps unacknowledged product analytics paused for this process. Existing
/// diagnostics retain their independently persisted behavior.
#[tauri::command]
pub async fn defer_privacy_consent_for_session() -> Result<(), String> {
    let _guard = CONSENT_MUTATION_LOCK.lock();
    product_analytics::disable();
    Ok(())
}

#[tauri::command]
pub async fn record_onboarding_completed() -> Result<(), String> {
    product_analytics::capture(product_analytics::ProductEvent::OnboardingCompleted);
    Ok(())
}

/// Bridges a frontend-reported error (e.g. from the React error boundary) into
/// the Rust Sentry client, where it is scrubbed by `before_send`. Gated on
/// consent and a no-op when telemetry is disabled.
#[tauri::command]
pub async fn report_frontend_error(name: Option<String>, message: String) -> Result<(), String> {
    telemetry::capture_frontend_error(name.as_deref(), &message);
    Ok(())
}
