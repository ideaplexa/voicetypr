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
        Self { client }
    }

    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        let adapter_kind = adapter_kind_for_provider(&request.provider_id)
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
        let model_str = namespaced_model(&request.provider_id, &request.model_id);
        let model = ModelIden::new(adapter_kind, model_str);
        let chat_request =
            ChatRequest::from_user(request.input_text.clone()).with_system(request.prompt.clone());

        // Inline formatting is latency- and cost-sensitive, so for reasoning-capable
        // models we ask for the cheapest reasoning effort the provider allows. Each
        // native adapter translates `ReasoningEffort::Minimal` into its own low knob:
        //   - OpenAI (gpt-5):  request body `reasoning_effort: "minimal"`
        //   - Gemini (2.5):    `thinkingConfig.thinkingBudget = 1000` (LOW; the genai
        //                      crate maps Minimal->LOW since a zero budget is rejected
        //                      by 2.5 Pro)
        //   - Anthropic (4.x): adaptive thinking, effort "low" / 1024 budget tokens
        // Non-reasoning models still receive sampling options, but never a
        // reasoning parameter a provider would reject (e.g. gpt-4o,
        // gemini-2.0-flash).
        let max_tokens = output_token_cap_for_input(request.input_text.len());
        let chat_options = if model_supports_reasoning(&request.provider_id, &request.model_id) {
            Some(
                ChatOptions::default()
                    .with_temperature(0.2)
                    .with_max_tokens(max_tokens)
                    .with_reasoning_effort(ReasoningEffort::Minimal),
            )
        } else {
            Some(
                ChatOptions::default()
                    .with_temperature(0.2)
                    .with_max_tokens(max_tokens),
            )
        };

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

/// Whether the embedded catalog flags `(provider_id, model_id)` as a reasoning
/// model. Unknown providers/models resolve to `false` so a reasoning effort is
/// only ever attached to calls that can accept it.
fn model_supports_reasoning(provider_id: &str, model_id: &str) -> bool {
    catalog::all_provider_models(provider_id)
        .into_iter()
        .any(|model| model.model_id == model_id && model.reasoning)
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
        assert!(model_supports_reasoning("openai", "gpt-5-mini"));
        assert!(model_supports_reasoning("gemini", "gemini-2.5-flash"));
        assert!(model_supports_reasoning("anthropic", "claude-haiku-4-5"));

        // Non-reasoning models must be excluded so no reasoning param is sent.
        assert!(!model_supports_reasoning("openai", "gpt-4o"));
        assert!(!model_supports_reasoning("gemini", "gemini-2.0-flash"));

        // Unknown provider/model resolves to false (safe default).
        assert!(!model_supports_reasoning("openai", "does-not-exist"));
        assert!(!model_supports_reasoning("custom", "anything"));
    }
}
