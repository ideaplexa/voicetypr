use reqwest::{header::HeaderMap, StatusCode};
use std::borrow::Cow;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiProviderError {
    #[error("missing API key")]
    MissingApiKey,
    #[error("invalid API key")]
    InvalidApiKey,
    #[error("invalid model")]
    InvalidModel,
    #[error("unsupported provider")]
    UnsupportedProvider,
    #[error("timed out")]
    Timeout,
    #[error("canceled")]
    Canceled,
    #[error("rate limited")]
    RateLimited,
    #[error("service unavailable")]
    ServiceUnavailable,
    #[error("network error")]
    Network,
    #[error("bad response")]
    BadResponse,
    #[error("internal error")]
    Internal,
    /// A local agent-CLI (e.g. Claude Code, pi, omp) reported a fatal outcome
    /// carrying its OWN message (e.g. "Not logged in · Please run /login").
    /// Surfaced verbatim to the user so the exact fix comes from the CLI itself,
    /// and it is never returned as polished text (parse_claude_json /
    /// parse_pi_jsonl map it to Err).
    #[error("{0}")]
    AgentCli(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MappedAiProviderError {
    pub error: AiProviderError,
    pub retry_after: Option<Duration>,
}

impl MappedAiProviderError {
    pub(crate) fn new(error: AiProviderError) -> Self {
        Self {
            error,
            retry_after: None,
        }
    }
}

pub fn user_facing_message(error: &AiProviderError) -> Cow<'static, str> {
    match error {
        AiProviderError::MissingApiKey => Cow::Borrowed("missing API key"),
        AiProviderError::InvalidApiKey => Cow::Borrowed("invalid API key"),
        AiProviderError::InvalidModel => Cow::Borrowed("invalid model"),
        AiProviderError::UnsupportedProvider => Cow::Borrowed("unsupported provider"),
        AiProviderError::Timeout => Cow::Borrowed("timed out"),
        AiProviderError::Canceled => Cow::Borrowed("canceled"),
        AiProviderError::RateLimited => Cow::Borrowed("rate limited"),
        AiProviderError::ServiceUnavailable => Cow::Borrowed("service unavailable"),
        AiProviderError::Network => Cow::Borrowed("network error"),
        AiProviderError::BadResponse => Cow::Borrowed("bad response"),
        AiProviderError::Internal => Cow::Borrowed("internal error"),
        // Surface the CLI's OWN message verbatim so the user learns the exact
        // fix from the CLI itself (claude/pi/omp each print their own guidance).
        AiProviderError::AgentCli(message) => Cow::Owned(message.clone()),
    }
}

pub(crate) fn map_http_status(
    status: StatusCode,
    body: Option<&str>,
    headers: Option<&HeaderMap>,
) -> MappedAiProviderError {
    let body = body.unwrap_or_default().to_ascii_lowercase();
    let error = match status.as_u16() {
        401 | 403 => AiProviderError::InvalidApiKey,
        400 if body.contains("api_key_invalid") || body.contains("api key not valid") => {
            AiProviderError::InvalidApiKey
        }
        429 => AiProviderError::RateLimited,
        // Request Timeout and Too Early are transient retry cases.
        408 | 425 => AiProviderError::ServiceUnavailable,
        400 | 404 if body.contains("model") => AiProviderError::InvalidModel,
        500..=599 => AiProviderError::ServiceUnavailable,
        _ => AiProviderError::BadResponse,
    };
    MappedAiProviderError {
        error,
        retry_after: headers.and_then(retry_after_from_headers),
    }
}

pub(crate) fn map_reqwest_error(error: &reqwest::Error) -> AiProviderError {
    if error.is_timeout() {
        return AiProviderError::Timeout;
    }
    if error.is_connect() || error.is_request() {
        return AiProviderError::Network;
    }
    AiProviderError::Network
}

pub(crate) fn map_genai_error(error: &genai::Error) -> MappedAiProviderError {
    match error {
        genai::Error::RequiresApiKey { .. }
        | genai::Error::NoAuthResolver { .. }
        | genai::Error::NoAuthData { .. } => {
            MappedAiProviderError::new(AiProviderError::MissingApiKey)
        }
        genai::Error::Resolver { .. } | genai::Error::ModelMapperFailed { .. } => {
            MappedAiProviderError::new(AiProviderError::MissingApiKey)
        }
        genai::Error::WebModelCall { webc_error, .. }
        | genai::Error::WebAdapterCall { webc_error, .. } => map_webc_error(webc_error),
        genai::Error::HttpError { status, body, .. } => map_http_status(*status, Some(body), None),
        genai::Error::ChatResponseGeneration { .. }
        | genai::Error::ChatResponse { .. }
        | genai::Error::NoChatResponse { .. }
        | genai::Error::InvalidJsonResponseElement { .. } => {
            MappedAiProviderError::new(AiProviderError::BadResponse)
        }
        _ => MappedAiProviderError::new(AiProviderError::Internal),
    }
}

fn map_webc_error(error: &genai::webc::Error) -> MappedAiProviderError {
    match error {
        genai::webc::Error::ResponseFailedStatus {
            status,
            body,
            headers,
        } => map_http_status(*status, Some(body), Some(headers.as_ref())),
        genai::webc::Error::Reqwest(error) => MappedAiProviderError::new(map_reqwest_error(error)),
        genai::webc::Error::ResponseFailedNotJson { .. }
        | genai::webc::Error::ResponseFailedInvalidJson { .. }
        | genai::webc::Error::JsonValueExt(_) => {
            MappedAiProviderError::new(AiProviderError::BadResponse)
        }
    }
}

fn retry_after_from_headers(headers: &HeaderMap) -> Option<Duration> {
    let value = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())?;

    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    chrono::DateTime::parse_from_rfc2822(value)
        .ok()
        .map(|retry_at| {
            retry_at
                .with_timezone(&chrono::Utc)
                .signed_duration_since(chrono::Utc::now())
                .to_std()
                .unwrap_or(Duration::ZERO)
        })
}

#[cfg(test)]
mod tests {
    use super::{map_http_status, AiProviderError};
    use reqwest::StatusCode;

    #[test]
    fn request_timeout_maps_to_retryable_transient_error() {
        let mapped = map_http_status(StatusCode::REQUEST_TIMEOUT, None, None);

        assert_eq!(mapped.error, AiProviderError::ServiceUnavailable);
    }
}
