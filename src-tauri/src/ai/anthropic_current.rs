//! Raw Messages API client for the Claude 4.7+/5.x lineup.
//!
//! genai 0.6's Anthropic adapter only knows effort for opus-4-5/4-6 and sends
//! legacy `enabled`/`budget_tokens` thinking to every other reasoning model.
//! Claude Opus 4.7+, Opus/Sonnet/Haiku 5.x and Fable reject that shape: they
//! need `output_config.effort` plus `thinking: {"type": "adaptive"}` (or
//! `between_tools` / `disabled`). These models route here; older Claude
//! models stay on the genai adapter.
use super::contract::{output_token_cap_for_input, thinking_token_allowance, AiPolishRequest};
use super::error::{map_http_status, map_reqwest_error, AiProviderError, MappedAiProviderError};
use super::genai_runtime::AiKeyResolver;
use serde_json::{json, Value};

/// Beta header that opts an account into `speed: "fast"` on the Messages API.
/// https://platform.claude.com/docs/en/build-with-claude/fast-mode
const FAST_MODE_BETA_HEADER: &str = "fast-mode-2026-02-01";

/// Models routed through this client instead of the genai adapter. They all
/// accept `output_config.effort` and adaptive (or disabled) thinking.
const RAW_MESSAGES_MODELS: &[&str] = &[
    "claude-haiku-5-5",
    "claude-sonnet-5-5",
    "claude-opus-5-5",
    "claude-sonnet-5",
    "claude-opus-5",
    "claude-fable-5",
    "claude-fable-5-1",
    "claude-opus-4-7",
    "claude-opus-4-8",
];

/// Whether an Anthropic model id routes through the raw Messages client.
pub fn uses_raw_messages(model_id: &str) -> bool {
    RAW_MESSAGES_MODELS.contains(&model_id)
}

#[derive(Clone)]
pub struct AnthropicCurrentRuntime {
    pub client: reqwest::Client,
    pub key_resolver: AiKeyResolver,
    pub base_url: String,
}

impl AnthropicCurrentRuntime {
    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        let key = (self.key_resolver)("anthropic")
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::MissingApiKey))?;
        let effort = super::catalog::resolve_reasoning_level(
            "anthropic",
            &request.model_id,
            request.reasoning_level.as_deref(),
        );
        // Sonnet 5.5 rejects disabled/legacy budgets; between_tools is its
        // lowest setting when the caller asked for (or resolved down to) a
        // level below low. https://platform.claude.com/docs/en/build-with-claude/thinking
        let thinking = if effort.as_deref() == Some("none") {
            json!({"type": "disabled"})
        } else if request.model_id == "claude-sonnet-5-5"
            && request
                .reasoning_level
                .as_deref()
                .is_none_or(|level| matches!(level, "off" | "none" | "minimal"))
        {
            json!({"type": "between_tools"})
        } else {
            json!({"type": "adaptive"})
        };
        let max_tokens = output_token_cap_for_input(
            request.input_text.len(),
            thinking_token_allowance(effort.as_deref()),
            super::catalog::find_model("anthropic", &request.model_id)
                .and_then(|model| model.output),
        );
        let mut payload = json!({
            "model": request.model_id,
            "system": request.prompt,
            "messages": [{"role": "user", "content": request.input_text}],
            "max_tokens": max_tokens,
            "thinking": thinking,
        });
        // "none" means thinking is off entirely — no effort field is sent.
        // https://platform.claude.com/docs/en/build-with-claude/effort
        if let Some(effort) = effort.as_deref().filter(|effort| *effort != "none") {
            payload["output_config"] = json!({"effort": effort});
        }
        if request.fast_mode {
            payload["speed"] = json!("fast");
        }

        let mut builder = self
            .client
            .post(format!("{}/messages", self.base_url.trim_end_matches('/')))
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload);
        if request.fast_mode {
            builder = builder.header("anthropic-beta", FAST_MODE_BETA_HEADER);
        }
        let response = builder
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
        // A truncated or refused response is partial: pasting it would
        // silently inject an incomplete rewrite (or a refusal notice) at the
        // cursor, so every non-natural stop reason becomes a retryable
        // BadResponse rather than delivered text.
        let stop_reason = value.get("stop_reason").and_then(Value::as_str);
        if matches!(
            stop_reason,
            Some("max_tokens" | "model_context_window_exceeded" | "refusal")
        ) {
            return Err(MappedAiProviderError::new(AiProviderError::BadResponse));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routed_models_cover_every_raw_messages_entry() {
        for model in RAW_MESSAGES_MODELS {
            assert!(uses_raw_messages(model), "{model} should route raw");
        }
        for model in ["claude-haiku-4-5", "claude-sonnet-4-5", "claude-opus-4-6"] {
            assert!(!uses_raw_messages(model), "{model} should stay on genai");
        }
    }
}
