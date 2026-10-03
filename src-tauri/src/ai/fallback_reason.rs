//! Closed fixture metadata. Unknown values never reach reports or errors.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FallbackReason {
    MissingApiKey,
    InvalidApiKey,
    InvalidModel,
    UnsupportedProvider,
    Timeout,
    Canceled,
    RateLimited,
    ServiceUnavailable,
    Network,
    BadResponse,
    Internal,
    MetaReply,
    Answered,
    AgentCli,
}
impl FallbackReason {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::MissingApiKey => "missing_api_key",
            Self::InvalidApiKey => "invalid_api_key",
            Self::InvalidModel => "invalid_model",
            Self::UnsupportedProvider => "unsupported_provider",
            Self::Timeout => "timeout",
            Self::Canceled => "canceled",
            Self::RateLimited => "rate_limited",
            Self::ServiceUnavailable => "service_unavailable",
            Self::Network => "network",
            Self::BadResponse => "bad_response",
            Self::Internal => "internal",
            Self::MetaReply => "meta_reply",
            Self::Answered => "answered",
            Self::AgentCli => "agent_cli",
        }
    }
    pub(crate) fn from_code(code: &str) -> Result<Self, &'static str> {
        serde_json::from_value(serde_json::Value::String(code.to_string()))
            .map_err(|_| "Invalid fallback reason")
    }
}
