use super::contract::{output_token_cap_for_input, AiPolishRequest};
use super::error::{map_genai_error, AiProviderError, MappedAiProviderError};
use crate::ai::catalog;
use genai::adapter::AdapterKind;
use genai::chat::{ChatOptions, ChatRequest, ReasoningEffort};
use genai::resolver::{AuthData, Endpoint};
use genai::{Client, ClientBuilder, ModelIden, ServiceTarget};
use std::collections::HashMap;
use std::sync::Arc;

pub type AiKeyResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

#[derive(Clone)]
pub struct GenaiRuntime {
    client: Client,
    anthropic_current: super::anthropic_current::AnthropicCurrentRuntime,
}

impl GenaiRuntime {
    pub fn with_endpoint_overrides(
        reqwest_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        endpoint_overrides: HashMap<String, String>,
    ) -> Self {
        let anthropic_current = super::anthropic_current::AnthropicCurrentRuntime {
            client: reqwest_client.clone(),
            key_resolver: key_resolver.clone(),
            base_url: endpoint_overrides
                .get("anthropic")
                .cloned()
                .unwrap_or_else(|| "https://api.anthropic.com/v1".to_string()),
        };
        let endpoint_overrides = Arc::new(endpoint_overrides);
        let auth_resolver = key_resolver.clone();
        let endpoint_resolver = endpoint_overrides.clone();
        let client = ClientBuilder::default()
            .with_reqwest(reqwest_client)
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
        Self {
            client,
            anthropic_current,
        }
    }

    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        if request.provider_id == "anthropic"
            && matches!(
                request.model_id.as_str(),
                "claude-sonnet-5-5"
                    | "claude-opus-5-5"
                    | "claude-sonnet-5"
                    | "claude-opus-5"
                    | "claude-fable-5"
                    | "claude-fable-5-1"
                    | "claude-opus-4-7"
                    | "claude-opus-4-8"
            )
        {
            return self.anthropic_current.polish(request).await;
        }
        let adapter_kind = adapter_kind_for_provider(&request.provider_id)
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
        let model_str = namespaced_model(&request.provider_id, &request.model_id);
        let model = ModelIden::new(adapter_kind, model_str);
        let chat_request =
            ChatRequest::from_user(request.input_text.clone()).with_system(request.prompt.clone());

        let chat_options = chat_options(request);

        let response = self
            .client
            .exec_chat(model, chat_request, chat_options.as_ref())
            .await
            .map_err(|error| map_genai_error(&error))?;

        response
            .into_first_text()
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::BadResponse))
    }
}

fn chat_options(request: &AiPolishRequest) -> Option<ChatOptions> {
    let max_tokens = output_token_cap_for_input(request.input_text.len());
    let mut options = ChatOptions::default();
    let model = catalog::all_provider_models(&request.provider_id)
        .into_iter()
        .find(|model| model.model_id == request.model_id);
    if model.is_none_or(|model| model.temperature) {
        options = options.with_temperature(0.2);
    }
    // genai 0.6's token-name table only knows GPT-5 and o-series.
    if request.provider_id == "openai" && request.model_id.starts_with("gpt-6") {
        options = options.with_extra_body(serde_json::json!({"max_completion_tokens": max_tokens}));
    } else {
        options = options.with_max_tokens(max_tokens);
    }
    if let Some(effort) = catalog::reasoning_effort(
        &request.provider_id,
        &request.model_id,
        request.reasoning_level.as_deref(),
    ) {
        let effort = match effort.as_str() {
            "none" => ReasoningEffort::None,
            "low" => ReasoningEffort::Low,
            "medium" => ReasoningEffort::Medium,
            "high" => ReasoningEffort::High,
            "xhigh" => ReasoningEffort::XHigh,
            "max" => ReasoningEffort::Max,
            _ => ReasoningEffort::Minimal,
        };
        options = options.with_reasoning_effort(effort);
    }
    Some(options)
}

fn adapter_kind_for_provider(provider_id: &str) -> Option<AdapterKind> {
    match catalog::adapter_name(provider_id)? {
        "OpenAI" => Some(AdapterKind::OpenAI),
        "Anthropic" => Some(AdapterKind::Anthropic),
        "Gemini" => Some(AdapterKind::Gemini),
        _ => None,
    }
}

fn provider_id_for_adapter(adapter_kind: AdapterKind) -> Option<&'static str> {
    let adapter_name = match adapter_kind {
        AdapterKind::OpenAI => "OpenAI",
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
        assert_eq!(namespaced_model("openai", "gpt-6.1-sol"), "gpt-6.1-sol");
        assert_eq!(
            namespaced_model("anthropic", "claude-sonnet-5-5"),
            "claude-sonnet-5-5"
        );
        assert_eq!(
            namespaced_model("gemini", "gemini-3.8-flash"),
            "gemini-3.8-flash"
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
    fn reasoning_effort_targets_only_reasoning_models() {
        // Recommended reasoning models should be gated in for minimal effort.
        assert!(catalog::reasoning_effort("openai", "gpt-6.1-sol", None).is_some());
        assert!(catalog::reasoning_effort("gemini", "gemini-3.8-flash", None).is_some());
        assert!(catalog::reasoning_effort("anthropic", "claude-sonnet-5-5", None).is_some());

        // Non-reasoning models must be excluded so no reasoning param is sent.
        assert!(catalog::reasoning_effort("openai", "gpt-4o", None).is_none());
        assert!(catalog::reasoning_effort("gemini", "gemini-2.0-flash", None).is_none());

        // Unknown provider/model resolves to false (safe default).
        assert!(catalog::reasoning_effort("openai", "does-not-exist", None).is_none());
        assert!(catalog::reasoning_effort("custom", "anything", None).is_none());
    }
}
