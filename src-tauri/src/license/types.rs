use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LicenseStatus {
    pub status: LicenseState,
    pub trial_days_left: Option<i32>,
    pub license_type: Option<String>,
    pub license_key: Option<String>,
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_state: Option<LicenseVerificationState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_expires_at: Option<DateTime<Utc>>,
}

impl LicenseStatus {
    pub fn verification_window_expired(&self, now: DateTime<Utc>) -> bool {
        matches!(self.status, LicenseState::Licensed)
            && self
                .verification_expires_at
                .is_some_and(|deadline| now >= deadline)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LicenseState {
    Licensed,
    Trial,
    Expired,
    None,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LicenseVerificationState {
    Verified,
    OfflineGrace,
    NeedsRevalidation,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrialCheckResponse {
    pub success: bool,
    pub data: TrialData,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TrialData {
    pub is_expired: bool,
    pub days_left: Option<i32>,
    pub expires_at: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LicenseValidateResponse {
    pub success: bool,
    pub data: ValidateData,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ValidateData {
    pub valid: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LicenseActivateResponse {
    pub success: bool,
    pub data: Option<ActivateData>,
    pub error: Option<String>,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ActivateData {
    pub activated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LicenseDeactivateResponse {
    pub success: bool,
    pub data: Option<DeactivateData>,
    pub error: Option<String>,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DeactivateData {
    pub deactivated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiError {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub message: String,
}
