use super::anthropic_current::{uses_raw_messages, AnthropicCurrentRuntime};
use super::contract::{output_token_cap_for_input, thinking_token_allowance, AiPolishRequest};
use super::error::{map_genai_error, AiProviderError, MappedAiProviderError};
use crate::ai::catalog;
use genai::adapter::AdapterKind;
use genai::chat::{ChatOptions, ChatRequest, ReasoningEffort, StopReason};
use genai::resolver::{AuthData, Endpoint};
use genai::{Client, ClientBuilder, ModelIden, ServiceTarget};
use std::collections::HashMap;
use std::sync::Arc;

pub type AiKeyResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const ANTHROPIC_DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1/";

#[derive(Clone)]
pub struct GenaiRuntime {
    client: Client,
    anthropic_current: AnthropicCurrentRuntime,
}

impl GenaiRuntime {
    pub fn with_endpoint_overrides(
        reqwest_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        endpoint_overrides: HashMap<String, String>,
    ) -> Self {
        let endpoint_overrides = Arc::new(endpoint_overrides);
        let auth_resolver = key_resolver.clone();
        let endpoint_resolver = endpoint_overrides.clone();
        let client = ClientBuilder::default()
            .with_reqwest(reqwest_client.clone())
            .with_auth_resolver_fn(move |model: ModelIden| {
                let provider_id = provider_id_for_adapter(model.adapter_kind).unwrap_or_default();
                Ok(auth_resolver(provider_id).map(AuthData::from_single))
            })
            .with_service_target_resolver_fn(move |mut target: ServiceTarget| {
                if let Some(provider_id) = provider_id_for_adapter(target.model.adapter_kind) {
                    if let Some(base_url) = endpoint_resolver.get(provider_id) {
                        target.endpoint = Endpoint::from_owned(ensure_trailing_slash(base_url));
                    }
                }
                Ok(target)
            })
            .build();
        let anthropic_base_url = endpoint_overrides
            .get("anthropic")
            .cloned()
            .unwrap_or_else(|| ANTHROPIC_DEFAULT_BASE_URL.to_string());
        Self {
            client,
            anthropic_current: AnthropicCurrentRuntime {
                client: reqwest_client,
                key_resolver,
                base_url: anthropic_base_url,
            },
        }
    }

    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        // Claude 4.7+/5.x models reject genai's legacy thinking shape; they go
        // through the raw Messages client (anthropic_current.rs).
        if request.provider_id == "anthropic" && uses_raw_messages(&request.model_id) {
            return self.anthropic_current.polish(request).await;
        }

        let adapter_kind = adapter_kind_for_provider(&request.provider_id)
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
        let model_str = namespaced_model(&request.provider_id, &request.model_id);
        let model = ModelIden::new(adapter_kind, model_str);
        let chat_request =
            ChatRequest::from_user(request.input_text.clone()).with_system(request.prompt.clone());

        let chat_options = Some(chat_options(request));

        let response = self
            .client
            .exec_chat(model, chat_request, chat_options.as_ref())
            .await
            .map_err(|error| map_genai_error(&error))?;

        // A max-tokens stop means the response is partial; pasting it would
        // silently inject a truncated rewrite at the cursor.
        if matches!(response.stop_reason, Some(StopReason::MaxTokens(_))) {
            return Err(MappedAiProviderError::new(AiProviderError::BadResponse));
        }
        response
            .into_first_text()
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))
    }
}

fn adapter_kind_for_provider(provider_id: &str) -> Option<AdapterKind> {
    match catalog::adapter_name(provider_id)? {
        "OpenAI" => Some(AdapterKind::OpenAI),
        "OpenAIResp" => Some(AdapterKind::OpenAIResp),
        "Anthropic" => Some(AdapterKind::Anthropic),
        "Gemini" => Some(AdapterKind::Gemini),
        _ => None,
    }
}

fn provider_id_for_adapter(adapter_kind: AdapterKind) -> Option<&'static str> {
    let adapter_name = match adapter_kind {
        AdapterKind::OpenAI => "OpenAI",
        AdapterKind::OpenAIResp => "OpenAIResp",
        AdapterKind::Anthropic => "Anthropic",
        AdapterKind::Gemini => "Gemini",
        _ => return None,
    };
    catalog::provider_for_adapter(adapter_name)
}

fn namespaced_model(provider_id: &str, model_id: &str) -> String {
    match catalog::namespace(provider_id) {
        Some(namespace) => format!("{namespace}{model_id}"),
        None => model_id.to_string(),
    }
}

/// Map a resolved level onto genai's ReasoningEffort. Budget-only models
/// (Gemini 2.5, older Claude) take legacy budgets via keyword efforts; "none"
/// needs `thinkingBudget: 0` to actually disable thinking, which genai only
/// emits through the explicit `Budget(0)` variant.
fn genai_effort(provider_id: &str, model_id: &str, level: &str) -> ReasoningEffort {
    if level == "none" && catalog::is_budget_only_reasoning(provider_id, model_id) {
        return ReasoningEffort::Budget(0);
    }
    match level {
        "none" => ReasoningEffort::None,
        "minimal" => ReasoningEffort::Minimal,
        "low" => ReasoningEffort::Low,
        "high" => ReasoningEffort::High,
        // medium and anything unlisted.
        _ => ReasoningEffort::Medium,
    }
}

/// Inline formatting is latency- and cost-sensitive, so reasoning models get
/// the resolved effort and no temperature (reasoning APIs reject or ignore
/// one); non-reasoning and unknown models keep the low temperature and never
/// receive a reasoning parameter a provider would reject.
fn chat_options(request: &AiPolishRequest) -> ChatOptions {
    let model = catalog::find_model(&request.provider_id, &request.model_id);
    let effort = catalog::resolve_reasoning_level(
        &request.provider_id,
        &request.model_id,
        request.reasoning_level.as_deref(),
    );
    let max_tokens = output_token_cap_for_input(
        request.input_text.len(),
        thinking_token_allowance(effort.as_deref()),
        model.and_then(|model| model.output),
    );

    let mut options = ChatOptions::default().with_max_tokens(max_tokens);
    let reasoning_model = model.is_some_and(|model| model.reasoning);
    if !reasoning_model {
        options = options.with_temperature(0.2);
    }
    if let Some(effort) = effort {
        options = options.with_reasoning_effort(genai_effort(
            &request.provider_id,
            &request.model_id,
            &effort,
        ));
    }
    // Fast mode: genai's ServiceTier has no "fast" variant (and the Responses
    // API expects the literal service_tier field), so it rides extra_body —
    // merged last, it cannot conflict with anything the adapter sets.
    if request.fast_mode && request.provider_id == "openai" {
        options = options.with_extra_body(serde_json::json!({"service_tier": "fast"}));
    }
    options
}

fn ensure_trailing_slash(base_url: &str) -> String {
    if base_url.ends_with('/') {
        base_url.to_string()
    } else {
        format!("{base_url}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaced_model_leaves_native_adapters_clean() {
        assert_eq!(namespaced_model("openai", "gpt-5-mini"), "gpt-5-mini");
        assert_eq!(
            namespaced_model("anthropic", "claude-haiku-4-5"),
            "claude-haiku-4-5"
        );
        assert_eq!(
            namespaced_model("gemini", "gemini-2.5-flash"),
            "gemini-2.5-flash"
        );
    }

    #[test]
    fn adapter_kind_round_trips_to_provider_id() {
        for provider_id in ["openai", "anthropic", "gemini"] {
            let kind = adapter_kind_for_provider(provider_id)
                .unwrap_or_else(|| panic!("{provider_id} should map to a genai adapter"));
            assert_eq!(provider_id_for_adapter(kind), Some(provider_id));
        }
    }

    #[test]
    fn openai_uses_responses_adapter() {
        assert_eq!(
            adapter_kind_for_provider("openai"),
            Some(AdapterKind::OpenAIResp)
        );
    }

    #[test]
    fn native_providers_support_reasoning() {
        let providers = catalog::launch_providers();
        let supports = |id: &str| {
            providers
                .iter()
                .find(|provider| provider.id == id)
                .map(|provider| provider.supports_reasoning)
        };
        assert_eq!(supports("openai"), Some(true));
        assert_eq!(supports("anthropic"), Some(true));
        assert_eq!(supports("gemini"), Some(true));
    }

    #[test]
    fn budget_only_none_needs_explicit_zero_budget() {
        assert!(matches!(
            genai_effort("gemini", "gemini-2.5-flash", "none"),
            ReasoningEffort::Budget(0)
        ));
        // Effort-value models keep the None variant.
        assert!(matches!(
            genai_effort("openai", "gpt-6-luna", "none"),
            ReasoningEffort::None
        ));
    }
}
