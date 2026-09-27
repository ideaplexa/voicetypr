use super::types::*;
use reqwest;
use serde_json::json;
use std::time::Duration;
use sysinfo::System;
use tokio::time::sleep;

const API_TIMEOUT: Duration = Duration::from_secs(30);
const VALIDATION_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Base URL for the Voicetypr backend API.
/// Debug builds inherit `VOICETYPR_API_URL` from `.env`, else the local backend
/// on port 4000. Release builds use the production host with NO env override —
/// these endpoints carry license keys + device hashes, so an env-redirect would
/// be a credential-exfiltration vector.
fn get_api_base_url() -> String {
    #[cfg(debug_assertions)]
    {
        std::env::var("VOICETYPR_API_URL")
            .unwrap_or_else(|_| "http://localhost:4000/api/v1".to_string())
    }
    #[cfg(not(debug_assertions))]
    {
        "https://api.voicetypr.com/api/v1".to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseValidationFailureKind {
    Unavailable,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseValidationError {
    pub kind: LicenseValidationFailureKind,
    pub message: String,
}

impl LicenseValidationError {
    fn unavailable(message: String) -> Self {
        Self {
            kind: LicenseValidationFailureKind::Unavailable,
            message,
        }
    }

    fn rejected(message: String) -> Self {
        Self {
            kind: LicenseValidationFailureKind::Rejected,
            message,
        }
    }
}

pub struct LicenseApiClient {
    client: reqwest::Client,
    base_url: String,
}

impl LicenseApiClient {
    pub fn new() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(API_TIMEOUT)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            client,
            base_url: get_api_base_url(),
        })
    }

    /// Check trial status for a device
    pub async fn check_trial(&self, device_hash: &str) -> Result<TrialCheckResponse, String> {
        let url = format!("{}/trial/check", self.base_url);

        // Trial check doesn't need OS info, keeping it simple
        let response = self
            .client
            .post(&url)
            .json(&json!({
                "deviceHash": device_hash
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if response.status().is_success() {
            response
                .json::<TrialCheckResponse>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else {
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: Some("unknown_error".to_string()),
                message: "Failed to check trial status".to_string(),
            });
            Err(error.message)
        }
    }

    /// Validate a license key
    pub async fn validate_license(
        &self,
        license_key: &str,
        device_hash: &str,
        app_version: Option<&str>,
    ) -> Result<LicenseValidateResponse, LicenseValidationError> {
        let url = format!("{}/license/validate", self.base_url);

        let mut body = json!({
            "licenseKey": license_key,
            "deviceHash": device_hash
        });

        if let Some(version) = app_version {
            body["appVersion"] = json!(version);
        }

        #[cfg(target_os = "macos")]
        {
            body["osType"] = json!("macos");
        }
        #[cfg(target_os = "windows")]
        {
            body["osType"] = json!("windows");
        }
        #[cfg(target_os = "linux")]
        {
            body["osType"] = json!("linux");
        }

        if let Some(os_version) = System::os_version() {
            body["osVersion"] = json!(os_version);
        }

        let mut last_error =
            LicenseValidationError::unavailable("License validation failed".to_string());
        for attempt in 1..=MAX_RETRIES {
            let response = match self
                .client
                .post(&url)
                .timeout(VALIDATION_ATTEMPT_TIMEOUT)
                .json(&body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    last_error =
                        LicenseValidationError::unavailable(format!("Network error: {}", error));
                    if attempt < MAX_RETRIES {
                        log::warn!(
                            "License validation transport failure (attempt {}/{}); retrying",
                            attempt,
                            MAX_RETRIES
                        );
                        sleep(INITIAL_RETRY_DELAY * attempt).await;
                        continue;
                    }
                    break;
                }
            };

            let status = response.status();
            if status.is_success() {
                match response.json::<LicenseValidateResponse>().await {
                    Ok(parsed) => return Ok(parsed),
                    Err(error) => {
                        last_error = LicenseValidationError::unavailable(format!(
                            "Failed to parse response: {}",
                            error
                        ));
                        if attempt < MAX_RETRIES {
                            log::warn!(
                                "License validation response was malformed (attempt {}/{}); retrying",
                                attempt,
                                MAX_RETRIES
                            );
                            sleep(INITIAL_RETRY_DELAY * attempt).await;
                            continue;
                        }
                        break;
                    }
                }
            }

            let retryable = status.is_server_error()
                || status == reqwest::StatusCode::REQUEST_TIMEOUT
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
            let error = response.json::<ApiError>().await.unwrap_or(ApiError {
                success: false,
                error: Some("unknown_error".to_string()),
                message: "Failed to validate license".to_string(),
            });

            if !retryable {
                return Err(LicenseValidationError::rejected(error.message));
            }

            last_error = LicenseValidationError::unavailable(error.message);
            if attempt < MAX_RETRIES {
                log::warn!(
                    "License validation service unavailable (attempt {}/{}); retrying",
                    attempt,
                    MAX_RETRIES
                );
                sleep(INITIAL_RETRY_DELAY * attempt).await;
                continue;
            }
            break;
        }

        Err(last_error)
    }

    /// Activate a license key on a device
    pub async fn activate_license(
        &self,
        license_key: &str,
        device_hash: &str,
        app_version: Option<&str>,
    ) -> Result<LicenseActivateResponse, String> {
        let url = format!("{}/license/activate", self.base_url);

        let mut body = json!({
            "licenseKey": license_key,
            "deviceHash": device_hash
        });

        // Add app version if provided
        if let Some(version) = app_version {
            body["appVersion"] = json!(version);
        }

        // Add OS type based on compile target
        #[cfg(target_os = "macos")]
        {
            body["osType"] = json!("macos");
        }
        #[cfg(target_os = "windows")]
        {
            body["osType"] = json!("windows");
        }
        #[cfg(target_os = "linux")]
        {
            body["osType"] = json!("linux");
        }

        // Add OS version using sysinfo
        if let Some(os_version) = System::os_version() {
            body["osVersion"] = json!(os_version);
        }

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if response.status().is_success() {
            response
                .json::<LicenseActivateResponse>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else if response.status() == 400 {
            // Bad request - includes various activation errors from our API
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: Some("activation_failed".to_string()),
                message: "Failed to activate license".to_string(),
            });
            Ok(LicenseActivateResponse {
                success: false,
                data: None,
                error: error.error.clone(),
                message: Some(error.message),
            })
        } else {
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: Some("unknown_error".to_string()),
                message: "Failed to activate license".to_string(),
            });
            Err(error.message)
        }
    }

    /// Deactivate a license from a device
    pub async fn deactivate_license(
        &self,
        license_key: &str,
        device_hash: &str,
    ) -> Result<LicenseDeactivateResponse, String> {
        let url = format!("{}/license/deactivate", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&json!({
                "licenseKey": license_key,
                "deviceHash": device_hash
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if response.status().is_success() {
            response
                .json::<LicenseDeactivateResponse>()
                .await
                .map_err(|e| format!("Failed to parse response: {}", e))
        } else {
            let error: ApiError = response.json().await.unwrap_or(ApiError {
                success: false,
                error: Some("unknown_error".to_string()),
                message: "Failed to deactivate license".to_string(),
            });
            Err(error.message)
        }
    }
}

impl Default for LicenseApiClient {
    fn default() -> Self {
        match Self::new() {
            Ok(client) => client,
            Err(e) => {
                log::error!("Failed to create default API client: {}", e);
                Self {
                    client: reqwest::Client::new(),
                    base_url: get_api_base_url(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use warp::Filter;

    fn test_client(base_url: String) -> LicenseApiClient {
        LicenseApiClient {
            client: reqwest::Client::new(),
            base_url,
        }
    }

    #[test]
    fn test_api_client_creation() {
        let client = LicenseApiClient::new();
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn validation_retries_service_failures_then_succeeds() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_route = Arc::clone(&attempts);
        let route = warp::path!("license" / "validate")
            .and(warp::post())
            .map(move || {
                let attempt = attempts_for_route.fetch_add(1, Ordering::SeqCst) + 1;
                let (body, status) = if attempt < MAX_RETRIES as usize {
                    (
                        json!({
                            "success": false,
                            "error": "service_unavailable",
                            "message": "License service unavailable"
                        }),
                        warp::http::StatusCode::SERVICE_UNAVAILABLE,
                    )
                } else {
                    (
                        json!({
                            "success": true,
                            "data": { "valid": true },
                            "message": null
                        }),
                        warp::http::StatusCode::OK,
                    )
                };
                warp::reply::with_status(warp::reply::json(&body), status)
            });
        let (address, server) = warp::serve(route).bind_ephemeral(([127, 0, 0, 1], 0));
        let server_task = tokio::spawn(server);
        let client = test_client(format!("http://{}", address));

        let result = client
            .validate_license(
                "VT-TEST-LICENSE",
                "0000000000000000000000000000000000000000000000000000000000000000",
                Some("2.0.4"),
            )
            .await;

        server_task.abort();
        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), MAX_RETRIES as usize);
    }

    #[tokio::test]
    async fn validation_classifies_client_rejection_without_retrying() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_route = Arc::clone(&attempts);
        let route = warp::path!("license" / "validate")
            .and(warp::post())
            .map(move || {
                attempts_for_route.fetch_add(1, Ordering::SeqCst);
                warp::reply::with_status(
                    warp::reply::json(&json!({
                        "success": false,
                        "error": "license_revoked",
                        "message": "License revoked"
                    })),
                    warp::http::StatusCode::NOT_FOUND,
                )
            });
        let (address, server) = warp::serve(route).bind_ephemeral(([127, 0, 0, 1], 0));
        let server_task = tokio::spawn(server);
        let client = test_client(format!("http://{}", address));

        let result = client
            .validate_license(
                "VT-TEST-LICENSE",
                "0000000000000000000000000000000000000000000000000000000000000000",
                Some("2.0.4"),
            )
            .await;

        server_task.abort();
        let error = result.unwrap_err();
        assert_eq!(error.kind, LicenseValidationFailureKind::Rejected);
        assert_eq!(error.message, "License revoked");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
