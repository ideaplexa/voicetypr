use crate::license::{
    api_client::{LicenseApiClient, LicenseValidationFailureKind},
    device, keychain, LicenseState, LicenseStatus, LicenseVerificationState,
};
use crate::simple_cache::{self as scache, SetItemOptions};
use crate::AppState;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::time::Instant;
use tauri::{AppHandle, Manager};

/// Cached license status to avoid repeated API calls
/// Cache is valid for 6 hours to balance freshness with performance
#[derive(Clone, Debug)]
pub struct CachedLicense {
    pub status: LicenseStatus,
    cached_at: Instant,
}

impl CachedLicense {
    /// Cache duration of 6 hours as requested by user
    const CACHE_DURATION: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

    pub fn new(status: LicenseStatus) -> Self {
        Self {
            status,
            cached_at: Instant::now(),
        }
    }

    /// Check if this cache entry is still valid
    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < Self::CACHE_DURATION
    }

    /// Get age of this cache entry for logging
    pub fn age(&self) -> std::time::Duration {
        self.cached_at.elapsed()
    }
}

// Implement UnwindSafe traits for panic testing compatibility
impl UnwindSafe for CachedLicense {}
impl RefUnwindSafe for CachedLicense {}

/// A completed failed check is not an in-flight check. Keep it distinct from
/// both a missing entitlement and a successful status so recording can direct
/// the user to recovery without falling back to an expired trial.
#[derive(Clone, Debug, Default)]
pub enum RuntimeLicenseCache {
    #[default]
    Loading,
    Ready(CachedLicense),
    Failed,
}

impl RuntimeLicenseCache {
    fn from_check(result: &Result<LicenseStatus, String>) -> Self {
        match result {
            Ok(status) => Self::Ready(CachedLicense::new(status.clone())),
            Err(_) => Self::Failed,
        }
    }
}

// Wrapper for cached license status with metadata
#[derive(Serialize, Deserialize, Debug)]
struct CachedLicenseStatus {
    status: LicenseStatus,
    cached_at: DateTime<Utc>,
}

// Constants for cache and grace periods
const OFFLINE_GRACE_PERIOD_DAYS: i64 = 90; // 90 days offline grace for licensed users - generous for legitimate use
const TRIAL_OFFLINE_GRACE_PERIOD_DAYS: i64 = 1; // 1 day offline grace for trial users - prevents abuse while allowing temporary outages
const CACHE_TTL_HOURS: u64 = 8; // 8-hour cache TTL for both licensed and trial users
const LICENSE_CACHE_KEY: &str = "license_status";
const LAST_VALIDATION_KEY: &str = "last_license_validation";
const VALIDATION_ANCHOR_INITIALIZED_KEY: &str = "license_validation_anchor_initialized";
const LAST_TRIAL_VALIDATION_KEY: &str = "last_trial_validation"; // Tracks when trial was last validated online
const TRIAL_EXPIRES_KEY: &str = "trial_expires_at"; // Cache key for trial expiry date
const VALIDATION_FAILURES_KEY: &str = "license_validation_failures";
const REVALIDATION_WARNING_THRESHOLD: u32 = 3;

// Error message constants for consistency
const ERR_INVALID_LICENSE: &str = "Invalid license key format";

// Helper function to format duration for logging
fn format_duration(duration: &Duration) -> String {
    let days = duration.num_days();
    let hours = duration.num_hours() % 24;
    let minutes = duration.num_minutes() % 60;

    if days > 0 {
        format!("{} days, {} hours", days, hours)
    } else if hours > 0 {
        format!("{} hours, {} minutes", hours, minutes)
    } else {
        format!("{} minutes", minutes)
    }
}

// Helper function to convert hours to days with ceiling
fn hours_to_days(hours: i64) -> i32 {
    (hours as f64 / 24.0).ceil() as i32
}

fn last_successful_validation(app: &AppHandle) -> Option<DateTime<Utc>> {
    scache::get(app, LAST_VALIDATION_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn validation_anchor_initialized(app: &AppHandle) -> bool {
    scache::get(app, VALIDATION_ANCHOR_INITIALIZED_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or(false)
}

// Check if we're within the trial grace period
fn is_within_trial_grace_period(app: &AppHandle) -> Option<i64> {
    // let cache = app.cache();

    if let Ok(Some(timestamp_json)) = scache::get(app, LAST_TRIAL_VALIDATION_KEY) {
        if let Ok(last_validation) = serde_json::from_value::<DateTime<Utc>>(timestamp_json) {
            let elapsed = Utc::now().signed_duration_since(last_validation);
            let days_elapsed = elapsed.num_days();

            if days_elapsed < TRIAL_OFFLINE_GRACE_PERIOD_DAYS {
                return Some(TRIAL_OFFLINE_GRACE_PERIOD_DAYS - days_elapsed);
            }
        }
    }

    None
}

// Conservative license deletion check - only delete when absolutely certain
fn should_delete_invalid_license(error_msg: &str) -> bool {
    let msg_lower = error_msg.to_lowercase();

    // ONLY delete if the error explicitly says the license is invalid
    // Be extremely conservative - when in doubt, keep the license
    msg_lower.contains("invalid license")
        || msg_lower.contains("license not found")
        || msg_lower.contains("license expired")
        || msg_lower.contains("license revoked")
}

fn consecutive_validation_failures(app: &AppHandle) -> u32 {
    scache::get(app, VALIDATION_FAILURES_KEY)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or(0)
}

fn record_validation_failure(app: &AppHandle) -> u32 {
    let failures = consecutive_validation_failures(app).saturating_add(1);
    if let Err(error) = scache::set(
        app,
        VALIDATION_FAILURES_KEY,
        serde_json::to_value(failures).unwrap_or_default(),
        None,
    ) {
        log::warn!(
            "Failed to persist license validation failure count: {}",
            error
        );
    }
    failures
}

fn reset_validation_failures(app: &AppHandle) {
    if let Err(error) = scache::remove(app, VALIDATION_FAILURES_KEY) {
        log::warn!(
            "Failed to reset license validation failure count: {}",
            error
        );
    }
}

fn verification_state_for_failures(failures: u32) -> LicenseVerificationState {
    if failures >= REVALIDATION_WARNING_THRESHOLD {
        LicenseVerificationState::NeedsRevalidation
    } else {
        LicenseVerificationState::OfflineGrace
    }
}

/// Check the current license status
/// This checks license first (if stored), then falls back to trial
/// Forces fresh check on app start, then uses cache during session
async fn seed_runtime_license_cache(app: &AppHandle, status: &LicenseStatus) {
    let app_state = app.state::<AppState>();
    let mut perf_cache = app_state.license_cache.write().await;
    *perf_cache = RuntimeLicenseCache::Ready(CachedLicense::new(status.clone()));
}

fn cache_license_status(app: &AppHandle, status: &LicenseStatus) {
    let wrapped_status = CachedLicenseStatus {
        status: status.clone(),
        cached_at: Utc::now(),
    };
    let cache_options = Some(SetItemOptions {
        ttl: Some(CACHE_TTL_HOURS * 60 * 60),
        compress: None,
        compression_method: None,
    });
    if let Err(error) = scache::set(
        app,
        LICENSE_CACHE_KEY,
        serde_json::to_value(wrapped_status).unwrap_or_default(),
        cache_options,
    ) {
        log::warn!("Failed to cache license status: {}", error);
    }
}

fn licensed_status(
    license_key: String,
    verification_state: LicenseVerificationState,
    expires_at: Option<String>,
    verification_expires_at: Option<DateTime<Utc>>,
) -> LicenseStatus {
    LicenseStatus {
        status: LicenseState::Licensed,
        trial_days_left: None,
        license_type: Some("pro".to_string()),
        license_key: Some(license_key),
        expires_at,
        verification_state: Some(verification_state),
        verification_expires_at,
    }
}

fn record_successful_validation(app: &AppHandle) -> DateTime<Utc> {
    let validation_time = Utc::now();
    if let Err(error) = scache::set(
        app,
        LAST_VALIDATION_KEY,
        serde_json::to_value(validation_time).unwrap_or_default(),
        None,
    ) {
        log::warn!("Failed to persist successful license validation: {}", error);
    }
    if let Err(error) = scache::set(
        app,
        VALIDATION_ANCHOR_INITIALIZED_KEY,
        serde_json::json!(true),
        None,
    ) {
        log::warn!(
            "Failed to persist license validation anchor state: {}",
            error
        );
    }
    reset_validation_failures(app);
    validation_time + Duration::days(OFFLINE_GRACE_PERIOD_DAYS)
}

fn preserve_paid_entitlement(
    app: &AppHandle,
    license_key: String,
) -> Result<LicenseStatus, String> {
    let now = Utc::now();
    let validation_time = match last_successful_validation(app) {
        Some(validation_time) => validation_time,
        None if validation_anchor_initialized(app) => {
            return Err(
                "Couldn't verify the license because its validation history is unavailable. Connect to the internet and revalidate."
                    .to_string(),
            );
        }
        None => {
            // Versions through 2.0.4 deleted this timestamp during every startup.
            // A stored key is only written after successful activation, so seed the
            // existing entitlement once instead of locking that paid user out.
            scache::set(
                app,
                LAST_VALIDATION_KEY,
                serde_json::to_value(now).unwrap_or_default(),
                None,
            )
            .map_err(|error| {
                format!(
                    "Couldn't preserve offline license access after verification failed: {}",
                    error
                )
            })?;
            scache::set(
                app,
                VALIDATION_ANCHOR_INITIALIZED_KEY,
                serde_json::json!(true),
                None,
            )
            .map_err(|error| {
                format!(
                    "Couldn't preserve the license validation boundary after verification failed: {}",
                    error
                )
            })?;
            now
        }
    };
    let verification_expires_at = validation_time + Duration::days(OFFLINE_GRACE_PERIOD_DAYS);
    if now >= verification_expires_at {
        return Err(
            "Couldn't verify the license and the offline grace period has ended. Connect to the internet and revalidate."
                .to_string(),
        );
    }

    let seconds_remaining = (verification_expires_at - now).num_seconds();
    let days_remaining = ((seconds_remaining + 86_399) / 86_400).max(1);
    let failures = record_validation_failure(app);
    let status = licensed_status(
        license_key,
        verification_state_for_failures(failures),
        Some(format!("{} days offline remaining", days_remaining)),
        Some(verification_expires_at),
    );
    cache_license_status(app, &status);
    Ok(status)
}

async fn revoke_paid_entitlement(app: &AppHandle) {
    if keychain::delete_license(app).is_err() {
        log::warn!("Failed to remove a definitively rejected license from secure storage");
    }
    if scache::set(
        app,
        VALIDATION_ANCHOR_INITIALIZED_KEY,
        serde_json::json!(true),
        None,
    )
    .is_err()
    {
        log::warn!("Failed to persist the rejected license boundary");
    }
    let _ = scache::remove(app, LAST_VALIDATION_KEY);
    reset_validation_failures(app);
    clear_license_status_cache(app).await;
}

#[tauri::command]
pub async fn check_license_status(app: AppHandle) -> Result<LicenseStatus, String> {
    let validation_lock = app.state::<AppState>().license_validation_lock.clone();
    let _validation_guard = validation_lock.lock().await;
    perform_license_status_check(&app).await
}

async fn perform_license_status_check(app: &AppHandle) -> Result<LicenseStatus, String> {
    log::info!("Checking license status");
    let result = check_license_status_impl(app.clone()).await;
    *app.state::<AppState>().license_cache.write().await = RuntimeLicenseCache::from_check(&result);
    result
}

/// Internal implementation of license status check
async fn check_license_status_impl(app: AppHandle) -> Result<LicenseStatus, String> {
    // let cache = app.cache();

    // Try to get cached status (cache is cleared on app start)
    match scache::get(&app, LICENSE_CACHE_KEY) {
        Ok(Some(cached_json)) => {
            log::info!("📦 Cache hit: Found cached license status");

            // Try to deserialize as new format first (with metadata)
            match serde_json::from_value::<CachedLicenseStatus>(cached_json.clone()) {
                Ok(cached_with_metadata) => {
                    let mut status = cached_with_metadata.status;
                    let cached_at = cached_with_metadata.cached_at;
                    let elapsed = Utc::now().signed_duration_since(cached_at);

                    log::info!(
                        "Cache hit: New format with metadata - cached {} ago",
                        format_duration(&elapsed)
                    );

                    // For trials, adjust days left based on elapsed time
                    if matches!(status.status, LicenseState::Trial) {
                        if let Some(original_days) = status.trial_days_left {
                            // Use ceiling for consistent day calculation (matches offline validation)
                            let hours_elapsed = elapsed.num_hours();
                            let elapsed_days = hours_to_days(hours_elapsed);
                            let current_days = (original_days - elapsed_days).max(0);

                            log::info!("Trial days adjustment: {} days cached - {} days elapsed = {} days left",
                                original_days, elapsed_days, current_days);

                            if current_days <= 0 {
                                log::warn!("Cached trial has expired - performing fresh check");
                                // Fall through to fresh check
                            } else {
                                status.trial_days_left = Some(current_days);
                                return Ok(status);
                            }
                        }
                    } else if status.verification_window_expired(Utc::now()) {
                        log::warn!(
                            "Cached offline grace has ended - performing fresh license check"
                        );
                    } else {
                        return Ok(status);
                    }
                }
                Err(_) => {
                    // Try old format (backward compatibility)
                    match serde_json::from_value::<LicenseStatus>(cached_json) {
                        Ok(cached_status) => {
                            log::info!(
                                "Cache hit: Old format (no metadata) - Type: {:?}, Days left: {:?}",
                                cached_status.status,
                                cached_status.trial_days_left
                            );

                            // For old format, we can't adjust trial days, so be conservative
                            if matches!(cached_status.status, LicenseState::Trial) {
                                if let Some(days) = cached_status.trial_days_left {
                                    if days <= 1 {
                                        log::warn!("Old format cache with low trial days - performing fresh check");
                                        // Fall through to fresh check
                                    } else {
                                        return Ok(cached_status);
                                    }
                                } else {
                                    return Ok(cached_status);
                                }
                            } else if cached_status.verification_window_expired(Utc::now()) {
                                log::warn!(
                                    "Cached offline grace has ended - performing fresh license check"
                                );
                            } else {
                                return Ok(cached_status);
                            }
                        }
                        Err(e) => {
                            log::warn!("Cache hit but failed to deserialize: {}", e);
                        }
                    }
                }
            }
        }
        Ok(None) => {
            log::info!("Cache miss: No cached license status found (fresh check after app start)");
        }
        Err(e) => {
            log::warn!("Cache error: Failed to check cache: {}", e);
        }
    }

    // Get device hash
    let device_hash = device::get_device_hash()?;

    // First, check if we have a stored license
    if let Some(license_key) = keychain::get_license(&app)? {
        log::info!("Found stored license, validating...");

        // Try to validate the stored license
        let api_client = LicenseApiClient::new()?;
        let app_version = app.package_info().version.to_string();

        match api_client
            .validate_license(&license_key, &device_hash, Some(&app_version))
            .await
        {
            Ok(response) if response.data.valid => {
                log::info!("License is valid");
                let verification_expires_at = record_successful_validation(&app);
                let status = licensed_status(
                    license_key,
                    LicenseVerificationState::Verified,
                    None,
                    Some(verification_expires_at),
                );
                cache_license_status(&app, &status);
                return Ok(status);
            }
            Ok(response) => {
                let message = response.message.as_deref().unwrap_or_default();
                if should_delete_invalid_license(message) {
                    log::warn!("License service definitively rejected the stored license");
                    revoke_paid_entitlement(&app).await;
                } else {
                    log::warn!(
                        "License could not be verified definitively; preserving paid entitlement"
                    );

                    if message.to_lowercase().contains("different device") {
                        log::info!("Attempting to repair the device activation");
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            activate_license_internal(license_key.clone(), app.clone()),
                        )
                        .await
                        {
                            Ok(Ok(status)) => return Ok(status),
                            Ok(Err(_)) => {
                                log::warn!(
                                    "Automatic device activation repair failed; preserving offline access"
                                );
                            }
                            Err(_) => {
                                log::warn!(
                                    "Automatic device activation repair timed out; preserving offline access"
                                );
                            }
                        }
                    }

                    return preserve_paid_entitlement(&app, license_key);
                }
            }
            Err(error)
                if error.kind == LicenseValidationFailureKind::Rejected
                    && should_delete_invalid_license(&error.message) =>
            {
                log::warn!("License service definitively rejected the stored license");
                revoke_paid_entitlement(&app).await;
            }
            Err(_) => {
                log::warn!(
                    "License verification unavailable after bounded retries; preserving offline access"
                );
                return preserve_paid_entitlement(&app, license_key);
            }
        }
    }

    // No valid license found, check trial status
    log::info!("Checking trial status");
    let api_client = LicenseApiClient::new()?;

    match api_client.check_trial(&device_hash).await {
        Ok(response) => {
            if response.data.is_expired {
                log::info!("Trial has expired");
                let status = LicenseStatus {
                    status: LicenseState::Expired,
                    trial_days_left: Some(0),
                    license_type: None,
                    license_key: None,
                    expires_at: None,
                    verification_state: None,
                    verification_expires_at: None,
                };

                // Don't cache expired status - always check
                log::info!("Not caching expired status - will check on every call");
                Ok(status)
            } else {
                // Backend now returns daysLeft!
                let trial_days_left = response.data.days_left.unwrap_or(0).max(0);

                log::info!("Trial is active with {} days left", trial_days_left);
                let status = LicenseStatus {
                    status: LicenseState::Trial,
                    trial_days_left: Some(trial_days_left),
                    license_type: None,
                    license_key: None,
                    expires_at: None,
                    verification_state: None,
                    verification_expires_at: None,
                };

                // Set last successful trial validation timestamp for grace period tracking
                let validation_time = Utc::now();
                if let Err(e) = scache::set(
                    &app,
                    LAST_TRIAL_VALIDATION_KEY,
                    serde_json::to_value(validation_time).unwrap_or_default(),
                    None, // No TTL for validation timestamp
                ) {
                    log::warn!("Failed to set trial validation timestamp: {}", e);
                } else {
                    log::info!("Set trial validation timestamp for grace period tracking");
                }

                // Cache the trial expiry date from server for offline validation
                if let Some(expires_at) = &response.data.expires_at {
                    log::info!("Caching trial expiry date from server: {}", expires_at);
                    let cache_options = Some(SetItemOptions {
                        ttl: Some(CACHE_TTL_HOURS * 60 * 60), // 8 hours TTL for trial cache
                        compress: None,
                        compression_method: None,
                    });
                    match scache::set(
                        &app,
                        TRIAL_EXPIRES_KEY,
                        serde_json::to_value(expires_at).unwrap_or_default(),
                        cache_options,
                    ) {
                        Ok(_) => log::info!("Cached trial expiry date for offline validation"),
                        Err(e) => log::error!("Failed to cache trial expiry date: {}", e),
                    }
                } else {
                    // No expires_at means something is wrong with device/trial creation
                    // Don't provide offline support - force online validation
                    log::error!("API returned trial without expires_at - this indicates a server issue. No offline support.");
                }

                // Only cache trial status if more than 1 day remaining
                // This prevents caching a trial that's about to expire
                if trial_days_left > 1 {
                    let wrapped_status = CachedLicenseStatus {
                        status: status.clone(),
                        cached_at: Utc::now(),
                    };

                    let cache_options = Some(SetItemOptions {
                        ttl: Some(CACHE_TTL_HOURS * 60 * 60), // 8 hours in seconds
                        compress: None,
                        compression_method: None,
                    });
                    match scache::set(
                        &app,
                        LICENSE_CACHE_KEY,
                        serde_json::to_value(&wrapped_status).unwrap_or_default(),
                        cache_options,
                    ) {
                        Ok(_) => log::info!(
                            "Cached trial status for {} hours - {} days remaining",
                            CACHE_TTL_HOURS,
                            trial_days_left
                        ),
                        Err(e) => log::error!("Failed to cache trial status: {}", e),
                    }
                } else {
                    log::info!(
                        "Not caching trial status - only {} days remaining (expires soon)",
                        trial_days_left
                    );
                }

                Ok(status)
            }
        }
        Err(e) => {
            log::error!("Failed to check trial status: {}", e);

            // FIRST: Check if we have a cached trial expiry date
            if let Ok(Some(expires_json)) = scache::get(&app, TRIAL_EXPIRES_KEY) {
                if let Ok(expires_at_str) = serde_json::from_value::<String>(expires_json) {
                    if let Ok(expires_at) = DateTime::parse_from_rfc3339(&expires_at_str) {
                        let expires_utc = expires_at.with_timezone(&Utc);
                        let now = Utc::now();

                        // Check if trial has already expired
                        if now >= expires_utc {
                            log::info!("Trial has expired (was valid until {}). No need to check grace period.", expires_at_str);
                            // Clear the expired trial cache
                            let _ = scache::remove(&app, TRIAL_EXPIRES_KEY);
                            let _ = scache::remove(&app, LAST_TRIAL_VALIDATION_KEY);

                            return Ok(LicenseStatus {
                                status: LicenseState::Expired,
                                trial_days_left: Some(0),
                                license_type: None,
                                license_key: None,
                                expires_at: None,
                                verification_state: None,
                                verification_expires_at: None,
                            });
                        }

                        // Trial is still valid, now check grace period
                        if let Some(grace_days_remaining) = is_within_trial_grace_period(&app) {
                            log::info!("Trial still valid AND within {}-day grace period. {} days grace remaining",
                                     TRIAL_OFFLINE_GRACE_PERIOD_DAYS, grace_days_remaining);

                            // Calculate actual trial days left
                            let hours_left = (expires_utc - now).num_hours();
                            let days_left = hours_to_days(hours_left).max(0);

                            log::info!(
                                "Offline trial access: {} trial days left (expires: {})",
                                days_left,
                                expires_at_str
                            );

                            let status = LicenseStatus {
                                status: if days_left > 0 {
                                    LicenseState::Trial
                                } else {
                                    LicenseState::Expired
                                },
                                trial_days_left: Some(days_left.max(0)),
                                license_type: None,
                                license_key: None,
                                expires_at: Some(format!(
                                    "Offline grace: {} day remaining",
                                    grace_days_remaining
                                )),
                                verification_state: None,
                                verification_expires_at: None,
                            };

                            return Ok(status);
                        } else {
                            log::error!("Trial is valid but grace period expired. Requires online validation.");
                            // Don't delete anything - user just needs to reconnect
                            return Err("Trial grace period expired. Please connect to internet to continue trial.".to_string());
                        }
                    } else {
                        log::warn!(
                            "Failed to parse cached trial expiry date: {}",
                            expires_at_str
                        );
                    }
                }
            } else {
                log::warn!("No cached trial expiry date found");
            }

            // No cached expiry or couldn't parse it - check grace period anyway (backward compatibility)
            if let Some(_grace_days_remaining) = is_within_trial_grace_period(&app) {
                log::warn!(
                    "No cached expiry but within grace period. This shouldn't happen normally."
                );
                return Err("Trial validation failed but grace period active. Please reconnect to internet.".to_string());
            }

            // Check if we have a cached license status to fall back on
            if let Ok(Some(cached_json)) = scache::get(&app, LICENSE_CACHE_KEY) {
                if let Ok(cached) = serde_json::from_value::<CachedLicenseStatus>(cached_json) {
                    let age = Utc::now().signed_duration_since(cached.cached_at);

                    // Use cached status if it's less than cache TTL
                    if age < Duration::hours(CACHE_TTL_HOURS as i64) {
                        log::info!(
                            "API unavailable, using cached license status from {} ago",
                            format_duration(&age)
                        );

                        // For trial users, adjust days remaining based on cache age
                        let mut status = cached.status.clone();
                        if status.status == LicenseState::Trial {
                            if let Some(days) = status.trial_days_left {
                                // Use ceiling for consistent day calculation
                                let hours_elapsed = age.num_hours();
                                let days_elapsed = hours_to_days(hours_elapsed);
                                status.trial_days_left = Some((days - days_elapsed).max(0));
                            }
                        }

                        return Ok(status);
                    }
                }
            }

            // No cached trial data - return no license status
            log::info!("No cached trial data - returning no license status");
            let status = LicenseStatus {
                status: LicenseState::None,
                trial_days_left: None,
                license_type: None,
                license_key: None,
                expires_at: None,
                verification_state: None,
                verification_expires_at: None,
            };

            // Don't cache None status - always check
            Ok(status)
        }
    }
}

/// Restore a license from keychain and validate it
#[tauri::command]
pub async fn restore_license(app: AppHandle) -> Result<LicenseStatus, String> {
    log::info!("Attempting to restore license");
    let validation_lock = app.state::<AppState>().license_validation_lock.clone();
    let _validation_guard = validation_lock.lock().await;

    // Check if we have a stored license
    let license_key =
        keychain::get_license(&app)?.ok_or_else(|| "No license found in keychain".to_string())?;

    let device_hash = device::get_device_hash()?;
    let api_client = LicenseApiClient::new()?;
    let app_version = app.package_info().version.to_string();

    // Try to validate the license
    match api_client
        .validate_license(&license_key, &device_hash, Some(&app_version))
        .await
    {
        Ok(response) => {
            if response.data.valid {
                log::info!("License restored successfully");

                // Clear cache when license is restored
                clear_license_status_cache(&app).await;

                let verification_expires_at = record_successful_validation(&app);

                // Reset recording state when license is restored
                let app_state = app.state::<AppState>();
                if let Err(e) = app_state.recording_state.reset() {
                    log::warn!(
                        "Failed to reset recording state during license restore: {}",
                        e
                    );
                } else {
                    log::info!("Reset recording state to Idle during license restore");
                }

                let status = licensed_status(
                    license_key,
                    LicenseVerificationState::Verified,
                    None,
                    Some(verification_expires_at),
                );
                seed_runtime_license_cache(&app, &status).await;
                Ok(status)
            } else {
                let message = response.message.as_deref().unwrap_or_default();
                if should_delete_invalid_license(message) {
                    revoke_paid_entitlement(&app).await;
                    Err(
                        "The stored license is no longer valid. Enter a valid license key to continue."
                            .to_string(),
                    )
                } else {
                    log::info!("License not valid for this device, attempting activation");
                    activate_license_internal(license_key, app).await
                }
            }
        }
        Err(error)
            if error.kind == LicenseValidationFailureKind::Rejected
                && should_delete_invalid_license(&error.message) =>
        {
            revoke_paid_entitlement(&app).await;
            Err(
                "The stored license is no longer valid. Enter a valid license key to continue."
                    .to_string(),
            )
        }
        Err(_) => {
            log::warn!("License restore verification unavailable; preserving offline access");
            let status = preserve_paid_entitlement(&app, license_key)?;
            seed_runtime_license_cache(&app, &status).await;
            let app_state = app.state::<AppState>();
            if let Err(error) = app_state.recording_state.reset() {
                log::warn!(
                    "Failed to reset recording state after offline license restore: {}",
                    error
                );
            }
            Ok(status)
        }
    }
}

/// Activate a new license key
#[tauri::command]
pub async fn activate_license(
    license_key: String,
    app: AppHandle,
) -> Result<LicenseStatus, String> {
    log::info!("Activating license");

    // Validate license key format (basic validation)
    let trimmed_key = license_key.trim();
    if trimmed_key.is_empty() {
        return Err("License key cannot be empty".to_string());
    }

    // Basic format validation: alphanumeric with hyphens, reasonable length
    if trimmed_key.len() < 10 || trimmed_key.len() > 100 {
        return Err(ERR_INVALID_LICENSE.to_string());
    }

    // Check for Voicetypr license format: must start with VT and contain hyphens
    if !trimmed_key.starts_with("VT") || !trimmed_key.contains('-') {
        return Err(ERR_INVALID_LICENSE.to_string());
    }

    // Check for valid characters (alphanumeric, hyphens, underscores)
    if !trimmed_key
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("License key contains invalid characters".to_string());
    }

    let validation_lock = app.state::<AppState>().license_validation_lock.clone();
    let _validation_guard = validation_lock.lock().await;

    // Reset recording state to Idle when activating license
    // This ensures we're not stuck in Error state from previous license issues
    let app_state = app.state::<AppState>();
    if let Err(e) = app_state.recording_state.reset() {
        log::warn!(
            "Failed to reset recording state during license activation: {}",
            e
        );
    } else {
        log::info!("Reset recording state to Idle during license activation");
    }

    activate_license_internal(trimmed_key.to_string(), app).await
}

/// Internal function to handle license activation
async fn activate_license_internal(
    license_key: String,
    app: AppHandle,
) -> Result<LicenseStatus, String> {
    let device_hash = device::get_device_hash()?;
    let api_client = LicenseApiClient::new()?;
    let app_version = app.package_info().version.to_string();

    match api_client
        .activate_license(&license_key, &device_hash, Some(&app_version))
        .await
    {
        Ok(response) => {
            if response.success {
                // Save the license to keychain
                keychain::save_license(&app, &license_key)?;

                // Immediately read it back to trigger macOS keychain permission prompt
                // This ensures the user grants permission during activation, not during first recording
                match keychain::get_license(&app)? {
                    Some(_) => log::info!("License saved and verified in keychain"),
                    None => {
                        log::error!("License was saved but could not be read back");
                        return Err("Failed to verify license storage".to_string());
                    }
                }

                log::info!("License activated successfully");

                // Clear cache when license is activated
                clear_license_status_cache(&app).await;

                let verification_expires_at = record_successful_validation(&app);

                // Reset recording state when license is successfully activated
                let app_state = app.state::<AppState>();
                if let Err(e) = app_state.recording_state.reset() {
                    log::warn!(
                        "Failed to reset recording state after successful activation: {}",
                        e
                    );
                } else {
                    log::info!("Reset recording state to Idle after successful activation");
                }

                let status = licensed_status(
                    license_key,
                    LicenseVerificationState::Verified,
                    None,
                    Some(verification_expires_at),
                );
                seed_runtime_license_cache(&app, &status).await;
                Ok(status)
            } else {
                // Return the actual error message from the API
                let error_msg = response
                    .message
                    .unwrap_or_else(|| "Failed to activate license".to_string());
                Err(error_msg)
            }
        }
        Err(e) => {
            log::error!("Failed to activate license: {}", e);
            Err(format!("Failed to activate license: {}", e))
        }
    }
}

/// Deactivate the current license
#[tauri::command]
pub async fn deactivate_license(app: AppHandle) -> Result<(), String> {
    log::info!("Deactivating license");
    let validation_lock = app.state::<AppState>().license_validation_lock.clone();
    let _validation_guard = validation_lock.lock().await;

    // Get the stored license
    let license_key =
        keychain::get_license(&app)?.ok_or_else(|| "No license found to deactivate".to_string())?;

    let device_hash = device::get_device_hash()?;
    let api_client = LicenseApiClient::new()?;

    match api_client
        .deactivate_license(&license_key, &device_hash)
        .await
    {
        Ok(response) => {
            if response.success {
                // Remove from keychain
                keychain::delete_license(&app)?;

                // Clear cache when license is deactivated
                // let cache = app.cache();
                match scache::remove(&app, LICENSE_CACHE_KEY) {
                    Ok(_) => log::info!("Cleared license cache after deactivation"),
                    Err(e) => log::warn!("Failed to clear cache after deactivation: {}", e),
                }
                // Clear validation timestamp when deactivating - this is intentional removal
                let _ = scache::remove(&app, LAST_VALIDATION_KEY);

                // Clear our performance cache too
                clear_license_status_cache(&app).await;
                reset_validation_failures(&app);

                log::info!("License deactivated successfully");

                // Reset recording state when license is deactivated
                let app_state = app.state::<AppState>();
                if let Err(e) = app_state.recording_state.reset() {
                    log::warn!("Failed to reset recording state after deactivation: {}", e);
                } else {
                    log::info!("Reset recording state to Idle after deactivation");
                }

                Ok(())
            } else {
                let error_msg = response
                    .message
                    .unwrap_or_else(|| "Failed to deactivate license".to_string());
                Err(error_msg)
            }
        }
        Err(e) => {
            log::error!("Failed to deactivate license: {}", e);
            // NEVER delete license on deactivation failure!
            // If deactivation fails, the server still thinks the license is active
            // User should be able to retry deactivation when they have proper connectivity
            log::warn!(
                "Deactivation failed. License remains in keychain. Please retry when connected."
            );

            // Clear cache even on failure
            // let cache = app.cache();
            match scache::remove(&app, LICENSE_CACHE_KEY) {
                Ok(_) => log::info!("Cleared license cache after deactivation failure"),
                Err(e) => log::warn!("Failed to clear cache after deactivation failure: {}", e),
            }
            // Keep the last validation timestamp even on failure
            // The deactivation failed, so everything should remain as-is

            // Reset recording state even on API failure (since we removed from keychain)
            let app_state = app.state::<AppState>();
            if let Err(e) = app_state.recording_state.reset() {
                log::warn!(
                    "Failed to reset recording state after deactivation failure: {}",
                    e
                );
            } else {
                log::info!("Reset recording state to Idle after deactivation failure");
            }

            Err(format!("Failed to deactivate license: {}", e))
        }
    }
}

/// Open the purchase page in the default browser
#[tauri::command]
pub async fn open_purchase_page() -> Result<(), String> {
    log::info!("Opening purchase page");

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("https://voicetypr.com/#pricing")
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        std::process::Command::new("cmd")
            .args(&["/C", "start", "https://voicetypr.com/#pricing"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg("https://voicetypr.com/#pricing")
            .spawn()
            .map_err(|e| format!("Failed to open browser: {}", e))?;
    }

    Ok(())
}

async fn clear_license_status_cache(app: &AppHandle) {
    match scache::remove(app, LICENSE_CACHE_KEY) {
        Ok(_) => log::debug!("Cleared cached license status"),
        Err(error) => log::warn!("Failed to clear cached license status: {}", error),
    }

    let app_state = app.state::<AppState>();
    let mut runtime_cache = app_state.license_cache.write().await;
    *runtime_cache = RuntimeLicenseCache::Loading;
}

#[tauri::command]
pub async fn revalidate_license(app: AppHandle) -> Result<LicenseStatus, String> {
    let validation_lock = app.state::<AppState>().license_validation_lock.clone();
    let _validation_guard = validation_lock.lock().await;
    clear_license_status_cache(&app).await;
    perform_license_status_check(&app).await
}

pub async fn check_license_status_internal(app: &AppHandle) -> Result<LicenseStatus, String> {
    check_license_status(app.clone()).await
}

/// Invalidate cached license status when license state changes
#[tauri::command]
pub async fn invalidate_license_cache(app: AppHandle) -> Result<(), String> {
    let validation_lock = app.state::<AppState>().license_validation_lock.clone();
    let _validation_guard = validation_lock.lock().await;
    clear_license_status_cache(&app).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_license_check_is_terminal_until_successful_recovery() {
        assert!(matches!(
            RuntimeLicenseCache::default(),
            RuntimeLicenseCache::Loading
        ));
        let failed = RuntimeLicenseCache::from_check(&Err(
            "Stored value for 'license' could not be decrypted; the saved entry was preserved"
                .to_string(),
        ));
        assert!(matches!(failed, RuntimeLicenseCache::Failed));

        let recovered = licensed_status(
            "VT-RECOVERED-TEST".to_string(),
            LicenseVerificationState::Verified,
            None,
            Some(Utc::now() + Duration::days(90)),
        );
        let ready = RuntimeLicenseCache::from_check(&Ok(recovered));
        let RuntimeLicenseCache::Ready(cached) = ready else {
            panic!("successful reactivation must replace the failed state");
        };
        assert_eq!(cached.status.status, LicenseState::Licensed);
        assert_eq!(
            cached.status.license_key.as_deref(),
            Some("VT-RECOVERED-TEST")
        );
        assert!(cached.is_valid());
    }

    #[test]
    fn repeated_verification_failures_escalate_without_expiring_entitlement() {
        assert_eq!(
            verification_state_for_failures(1),
            LicenseVerificationState::OfflineGrace
        );
        assert_eq!(
            verification_state_for_failures(2),
            LicenseVerificationState::OfflineGrace
        );
        assert_eq!(
            verification_state_for_failures(3),
            LicenseVerificationState::NeedsRevalidation
        );

        let deadline = Utc::now() + Duration::days(90);
        let status = licensed_status(
            "VT-TEST-LICENSE".to_string(),
            verification_state_for_failures(3),
            Some("90 days offline remaining".to_string()),
            Some(deadline),
        );
        assert_eq!(status.status, LicenseState::Licensed);
        assert_eq!(
            status.verification_state,
            Some(LicenseVerificationState::NeedsRevalidation)
        );
        assert!(!status.verification_window_expired(Utc::now()));
        assert!(status.verification_window_expired(deadline));
    }

    #[test]
    fn verified_license_requires_revalidation_at_exact_deadline() {
        let deadline = Utc::now() + Duration::days(90);
        let status = licensed_status(
            "VT-TEST-LICENSE".to_string(),
            LicenseVerificationState::Verified,
            None,
            Some(deadline),
        );

        assert!(!status.verification_window_expired(deadline - Duration::milliseconds(1)));
        assert!(status.verification_window_expired(deadline));
    }

    #[test]
    fn only_definitive_entitlement_messages_delete_a_stored_license() {
        assert!(should_delete_invalid_license("License revoked"));
        assert!(should_delete_invalid_license("License not found"));
        assert!(!should_delete_invalid_license(
            "This license is registered to a different device."
        ));
        assert!(!should_delete_invalid_license(
            "License service temporarily unavailable"
        ));
    }

    #[test]
    fn old_cached_license_status_deserializes_without_verification_metadata() {
        let status: LicenseStatus = serde_json::from_value(serde_json::json!({
            "status": "licensed",
            "trial_days_left": null,
            "license_type": "pro",
            "license_key": "VT-TEST-LICENSE",
            "expires_at": null
        }))
        .expect("legacy cached status should remain readable");

        assert_eq!(status.status, LicenseState::Licensed);
        assert_eq!(status.verification_state, None);
    }
}
