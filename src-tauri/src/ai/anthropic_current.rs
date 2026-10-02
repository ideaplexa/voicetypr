//! Claude 5.5 Messages support until genai's legacy model tables support it.
use super::contract::{output_token_cap_for_input, AiPolishRequest};
use super::error::{map_http_status, map_reqwest_error, AiProviderError, MappedAiProviderError};
use super::genai_runtime::AiKeyResolver;
use serde_json::{json, Value};

#[derive(Clone)]
pub(super) struct AnthropicCurrentRuntime {
    pub client: reqwest::Client,
    pub key_resolver: AiKeyResolver,
    pub base_url: String,
}

impl AnthropicCurrentRuntime {
    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        let key = (self.key_resolver)("anthropic")
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::MissingApiKey))?;
        let effort = super::catalog::reasoning_effort(
            "anthropic",
            &request.model_id,
            request.reasoning_level.as_deref(),
        )
        .unwrap_or_else(|| "low".to_string());
        // Sonnet 5.5 rejects disabled/legacy budgets; between_tools is its
        // lowest setting. Opus 5.5 requires adaptive thinking at every effort.
        // https://platform.claude.com/docs/en/build-with-claude/thinking
        let thinking = if request.model_id == "claude-sonnet-5-5"
            && matches!(effort.as_str(), "low" | "medium" | "high")
            && request
                .reasoning_level
                .as_deref()
                .is_none_or(|level| matches!(level, "off" | "none" | "minimal"))
        {
            "between_tools"
        } else {
            "adaptive"
        };
        let payload = json!({
            "model": request.model_id,
            "system": request.prompt,
            "messages": [{"role": "user", "content": request.input_text}],
            "max_tokens": output_token_cap_for_input(request.input_text.len()),
            "output_config": {"effort": effort},
            "thinking": {"type": thinking}
        });
        let response = self
            .client
            .post(format!("{}/messages", self.base_url.trim_end_matches('/')))
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()
            .await
            .map_err(|error| MappedAiProviderError::new(map_reqwest_error(&error)))?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response
            .text()
            .await
            .map_err(|error| MappedAiProviderError::new(map_reqwest_error(&error)))?;
        if !status.is_success() {
            return Err(map_http_status(status, Some(&body), Some(&headers)));
        }
        let value: Value = serde_json::from_str(&body)
            .map_err(|_| MappedAiProviderError::new(AiProviderError::BadResponse))?;
        let text = value
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|block| block["type"] == "text")
                    .filter_map(|block| block["text"].as_str())
                    .collect::<String>()
            })
            .filter(|text| !text.is_empty())
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))?;
        Ok(text)
    }
}
