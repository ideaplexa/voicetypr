use super::contract::{AiModel, AiProvider};
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct CatalogFile {
    pub providers: Vec<CatalogProvider>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogProvider {
    pub id: String,
    pub label: String,
    pub status: String,
    #[serde(default = "default_runtime")]
    pub runtime: String,
    pub adapter: Option<String>,
    pub namespace: Option<String>,
    pub requires_api_key: bool,
    pub supports_base_url: bool,
    pub supports_reasoning: bool,
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogModel {
    pub model_id: String,
    pub label: String,
    pub recommended: bool,
    pub reasoning: bool,
    /// Raw models.dev effort values (may include xhigh/max, never surfaced);
    /// empty for budget-only and non-reasoning models.
    #[serde(default)]
    pub reasoning_efforts: Vec<String>,
    #[serde(default)]
    pub reasoning_levels: Vec<String>,
    #[serde(default)]
    pub fast_mode: bool,
    pub output: Option<u64>,
    pub context: Option<u64>,
    pub cost_input: Option<f64>,
    pub cost_output: Option<f64>,
}

type Catalog = CatalogFile;

fn default_runtime() -> String {
    "genai_adapter".to_string()
}

// Project rule prefers LazyLock when the initializer is known at declaration time;
// this preserves the contract's parse-once behavior.
static CATALOG: LazyLock<Catalog> =
    LazyLock::new(|| parse_catalog(include_str!("../../catalog/catalog.generated.json")));

fn parse_catalog(json: &str) -> Catalog {
    let mut catalog = match serde_json::from_str::<Catalog>(json) {
        Ok(catalog) => catalog,
        Err(error) => {
            log::error!("embedded AI provider catalog failed to parse: {error}");
            return Catalog {
                providers: Vec::new(),
            };
        }
    };

    catalog.providers.push(CatalogProvider {
        id: "custom".to_string(),
        label: "Custom (OpenAI-compatible)".to_string(),
        status: "production".to_string(),
        runtime: "openai_compatible".to_string(),
        adapter: None,
        namespace: None,
        requires_api_key: false,
        supports_base_url: true,
        supports_reasoning: false,
        models: Vec::new(),
    });

    // Subscription-authenticated coding CLIs. Each request uses a fresh child
    // in an empty temporary directory (Claude may consume a prepared spare).
    // Provider-specific flags live in agent_cli.rs.
    for (id, label, status) in [
        ("claude-code", "Claude Code", "production"),
        ("pi", "pi", "production"),
        ("omp", "oh-my-pi", "production"),
        ("codex", "Codex", "production"),
        ("droid", "Droid", "production"),
        ("grok", "Grok", "production"),
        ("opencode", "OpenCode", "production"),
        ("cline", "Cline", "production"),
        // No headless polish run has been verified end-to-end yet.
        ("cursor", "Cursor", "experimental"),
    ] {
        catalog.providers.push(CatalogProvider {
            id: id.to_string(),
            label: label.to_string(),
            status: status.to_string(),
            runtime: "agent_cli".to_string(),
            adapter: None,
            namespace: None,
            requires_api_key: false,
            supports_base_url: false,
            supports_reasoning: false,
            models: Vec::new(),
        });
    }
    catalog
}

fn catalog() -> &'static Catalog {
    &CATALOG
}

fn provider(provider_id: &str) -> Option<&'static CatalogProvider> {
    catalog()
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
}

fn model(provider_id: &str, model: &CatalogModel) -> AiModel {
    AiModel {
        provider_id: provider_id.to_string(),
        model_id: model.model_id.clone(),
        label: model.label.clone(),
        recommended: model.recommended,
    }
}

pub fn launch_providers() -> Vec<AiProvider> {
    catalog()
        .providers
        .iter()
        .map(|provider| AiProvider {
            id: provider.id.clone(),
            label: provider.label.clone(),
            status: provider.status.clone(),
            requires_api_key: provider.requires_api_key,
            supports_base_url: provider.supports_base_url,
            supports_reasoning: provider.supports_reasoning,
        })
        .collect()
}

pub fn recommended_models(provider_id: &str) -> Vec<AiModel> {
    provider(provider_id)
        .map(|provider| {
            provider
                .models
                .iter()
                .filter(|model| model.recommended)
                .map(|entry| model(provider_id, entry))
                .collect()
        })
        .unwrap_or_default()
}

pub fn all_provider_models(provider_id: &str) -> Vec<&'static CatalogModel> {
    provider(provider_id)
        .map(|provider| provider.models.iter().collect())
        .unwrap_or_default()
}

/// Look up a model in a provider's catalog list.
pub fn find_model(provider_id: &str, model_id: &str) -> Option<&'static CatalogModel> {
    provider(provider_id).and_then(|provider| {
        provider
            .models
            .iter()
            .find(|model| model.model_id == model_id)
    })
}

/// The provider's default model: the first entry of the overlay's ordered
/// `recommended` list (generated catalog order preserves it).
pub fn default_model_id(provider_id: &str) -> Option<&'static str> {
    provider(provider_id)
        .and_then(|provider| provider.models.iter().find(|model| model.recommended))
        .map(|model| model.model_id.as_str())
}

/// Whether the provider exposes any model that supports the fast service tier.
pub fn provider_has_fast_models(provider_id: &str) -> bool {
    all_provider_models(provider_id)
        .iter()
        .any(|model| model.fast_mode)
}

/// Canonical selectable effort ladder, low→high. Catalog `reasoning_levels`
/// carry values in this order; xhigh/max are never surfaced.
pub const REASONING_LEVEL_ORDER: [&str; 5] = ["none", "minimal", "low", "medium", "high"];

/// Stored reasoning level used for native providers before the user picks one.
pub const DEFAULT_REASONING_LEVEL: &str = "medium";

/// Resolve a stored effort level against a model's supported levels.
///
/// Rule (shared with the frontend): use the stored level if the model supports
/// it; else "medium" if supported; else the highest supported level below
/// "medium"; else the lowest supported level; else no effort.
pub fn resolve_reasoning_level(
    provider_id: &str,
    model_id: &str,
    stored: Option<&str>,
) -> Option<String> {
    let model = find_model(provider_id, model_id)?;
    resolve_supported_reasoning_level(&model.reasoning_levels, stored)
}

fn resolve_supported_reasoning_level(levels: &[String], stored: Option<&str>) -> Option<String> {
    if levels.is_empty() {
        return None;
    }
    if let Some(stored) = stored {
        if levels.iter().any(|level| level == stored) {
            return Some(stored.to_string());
        }
    }
    if levels.iter().any(|level| level == "medium") {
        return Some("medium".to_string());
    }
    // Highest supported level below medium.
    for level in ["low", "minimal", "none"] {
        if levels.iter().any(|candidate| candidate == level) {
            return Some(level.to_string());
        }
    }
    // Lowest supported level.
    REASONING_LEVEL_ORDER
        .iter()
        .find(|level| levels.iter().any(|candidate| candidate == *level))
        .map(|level| (*level).to_string())
}

#[cfg(test)]
#[test]
fn reasoning_resolution_covers_sparse_ladders() {
    let resolve = |levels: &[&str], stored| {
        resolve_supported_reasoning_level(
            &levels
                .iter()
                .map(|level| (*level).to_string())
                .collect::<Vec<_>>(),
            stored,
        )
    };
    assert_eq!(
        resolve(&["none", "low", "high"], None).as_deref(),
        Some("low")
    );
    assert_eq!(
        resolve(&["minimal", "high"], None).as_deref(),
        Some("minimal")
    );
    assert_eq!(resolve(&["high"], Some("medium")).as_deref(), Some("high"));
    assert_eq!(resolve(&[], Some("medium")), None);
}

/// The lowest effort a model accepts — used for API-key validation probes so
/// they exercise the cheapest supported thinking level.
pub fn lowest_reasoning_level(provider_id: &str, model_id: &str) -> Option<String> {
    find_model(provider_id, model_id).and_then(|model| {
        REASONING_LEVEL_ORDER
            .iter()
            .find(|level| model.reasoning_levels.iter().any(|l| l == *level))
            .map(|level| (*level).to_string())
    })
}

/// Whether the model's reasoning came from a budget_tokens option only (no
/// named effort values). On genai those levels map to legacy budgets, and
/// "none" needs `thinkingBudget: 0` to actually disable thinking.
pub fn is_budget_only_reasoning(provider_id: &str, model_id: &str) -> bool {
    find_model(provider_id, model_id)
        .is_some_and(|model| model.reasoning && model.reasoning_efforts.is_empty())
}

pub fn is_native_provider(provider_id: &str) -> bool {
    provider(provider_id)
        .is_some_and(|provider| provider.runtime == "genai_adapter" && provider.adapter.is_some())
}

pub fn runtime_kind(provider_id: &str) -> Option<&'static str> {
    provider(provider_id).map(|provider| provider.runtime.as_str())
}

pub fn adapter_name(provider_id: &str) -> Option<&'static str> {
    provider(provider_id).and_then(|provider| provider.adapter.as_deref())
}

pub fn namespace(provider_id: &str) -> Option<&'static str> {
    provider(provider_id).and_then(|provider| provider.namespace.as_deref())
}

pub fn provider_for_adapter(adapter_name: &str) -> Option<&'static str> {
    catalog()
        .providers
        .iter()
        .find(|provider| provider.adapter.as_deref() == Some(adapter_name))
        .map(|provider| provider.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_has_expected_shape() {
        let catalog = catalog();
        let generated_provider_count = catalog
            .providers
            .iter()
            .filter(|provider| provider.id != "custom")
            .count();
        assert!(generated_provider_count >= 3);

        let mut provider_ids = HashSet::new();
        for provider in &catalog.providers {
            assert!(provider_ids.insert(provider.id.as_str()));
            if matches!(provider.status.as_str(), "production" | "experimental") {
                match provider.runtime.as_str() {
                    "genai_adapter" => assert!(provider
                        .adapter
                        .as_deref()
                        .is_some_and(|adapter| !adapter.is_empty())),
                    "openai_compatible" | "agent_cli" => assert!(provider.adapter.is_none()),
                    runtime => panic!("{} has unsupported runtime {runtime}", provider.id),
                }
            }

            let mut model_ids = HashSet::new();
            for model in &provider.models {
                assert!(model_ids.insert(model.model_id.as_str()));
            }

            let recommended_ids: HashSet<&str> = provider
                .models
                .iter()
                .filter(|model| model.recommended)
                .map(|model| model.model_id.as_str())
                .collect();
            for recommended in recommended_models(&provider.id) {
                assert!(recommended_ids.contains(recommended.model_id.as_str()));
            }
        }
    }

    #[test]
    fn malformed_catalog_json_parses_to_empty_catalog() {
        let catalog = parse_catalog("{\"providers\":[{\"id\":\"missing-fields\"}]}");

        assert!(catalog.providers.is_empty());
    }

    #[test]
    fn production_and_experimental_providers_have_valid_runtime_contracts() {
        for provider in &catalog().providers {
            if !matches!(provider.status.as_str(), "production" | "experimental") {
                continue;
            }

            match provider.runtime.as_str() {
                "genai_adapter" => {
                    assert!(
                        adapter_name(&provider.id).is_some(),
                        "{} should have a genai adapter",
                        provider.id
                    );
                }
                "openai_compatible" | "agent_cli" => {
                    assert!(
                        adapter_name(&provider.id).is_none(),
                        "{} should not have a genai adapter",
                        provider.id
                    );
                }
                runtime => panic!("{} has unsupported runtime {runtime}", provider.id),
            }
        }
    }

    #[test]
    fn adapter_to_provider_mapping_round_trips() {
        for provider in &catalog().providers {
            match provider.runtime.as_str() {
                "genai_adapter" => {
                    let adapter = adapter_name(&provider.id)
                        .unwrap_or_else(|| panic!("{} should have a genai adapter", provider.id));
                    assert_eq!(provider_for_adapter(adapter), Some(provider.id.as_str()));
                }
                "openai_compatible" | "agent_cli" => {
                    assert!(adapter_name(&provider.id).is_none());
                }
                runtime => panic!("{} has unsupported runtime {runtime}", provider.id),
            }
        }
    }

    #[test]
    fn recommended_models_are_subset_of_all_models() {
        for provider in &catalog().providers {
            let all_model_ids: HashSet<String> = all_provider_models(&provider.id)
                .into_iter()
                .map(|model| model.model_id.clone())
                .collect();

            for recommended in recommended_models(&provider.id) {
                assert!(
                    all_model_ids.contains(&recommended.model_id),
                    "{} recommended model {} should be in all models",
                    provider.id,
                    recommended.model_id
                );
            }
        }
    }

    #[test]
    fn overlay_recommended_models_exist_in_catalog() {
        let overlay: serde_json::Value =
            serde_json::from_str(include_str!("../../catalog/overlay.json"))
                .expect("overlay must be valid JSON");

        for provider in &catalog().providers {
            let Some(recommended) = overlay
                .get(&provider.id)
                .and_then(|provider| provider.get("recommended"))
                .and_then(serde_json::Value::as_array)
            else {
                continue;
            };

            let model_ids: HashSet<&str> = provider
                .models
                .iter()
                .map(|model| model.model_id.as_str())
                .collect();
            for model_id in recommended {
                let model_id = model_id
                    .as_str()
                    .expect("recommended model id must be a string");
                assert!(model_ids.contains(model_id));
            }
        }
    }

    #[test]
    fn agent_cli_providers_share_runtime_contract() {
        for id in [
            "claude-code",
            "pi",
            "omp",
            "codex",
            "droid",
            "grok",
            "opencode",
            "cline",
            "cursor",
        ] {
            assert_eq!(runtime_kind(id), Some("agent_cli"), "{id} runtime");
            assert!(!is_native_provider(id), "{id} not native");
            assert_eq!(adapter_name(id), None, "{id} no adapter");
            let provider = provider(id).unwrap_or_else(|| panic!("{id} must be in the catalog"));
            assert!(!provider.requires_api_key, "{id} no api key");
            assert!(!provider.supports_base_url, "{id} no base url");
            assert!(provider.models.is_empty(), "{id} empty models");
        }
        for retired in ["amp", "kilo-code"] {
            assert!(provider(retired).is_none(), "{retired} must stay retired");
        }
    }

    #[test]
    fn reasoning_levels_follow_the_contract_rule() {
        let levels = |provider: &str, model: &str| {
            find_model(provider, model)
                .unwrap_or_else(|| panic!("{provider}/{model} must exist"))
                .reasoning_levels
                .clone()
        };
        // Effort values ∩ the 5-level ladder; a toggle adds "none".
        assert_eq!(
            levels("openai", "gpt-6-luna"),
            vec!["none", "low", "medium", "high"]
        );
        assert_eq!(
            levels("openai", "gpt-5-mini"),
            vec!["minimal", "low", "medium", "high"]
        );
        assert_eq!(
            levels("anthropic", "claude-haiku-5-5"),
            vec!["none", "low", "medium", "high"]
        );
        assert_eq!(
            levels("anthropic", "claude-sonnet-5-5"),
            vec!["low", "medium", "high"]
        );
        // Budget-only models get low/medium/high, plus "none" when the
        // budget can be 0 (2.5 flash) or the model has a toggle.
        assert_eq!(
            levels("gemini", "gemini-2.5-flash"),
            vec!["none", "low", "medium", "high"]
        );
        assert_eq!(
            levels("anthropic", "claude-haiku-4-5"),
            vec!["low", "medium", "high"]
        );
        // Non-reasoning models expose no levels.
        assert!(levels("openai", "gpt-4o").is_empty());
    }

    #[test]
    fn resolve_reasoning_level_follows_the_contract_rule() {
        // Stored level wins when supported.
        assert_eq!(
            resolve_reasoning_level("openai", "gpt-6-luna", Some("high")),
            Some("high".to_string())
        );
        // Unsupported stored level falls back to medium when supported.
        assert_eq!(
            resolve_reasoning_level("openai", "gpt-5-mini", Some("none")),
            Some("medium".to_string())
        );
        // No medium: highest supported level below medium.
        assert_eq!(
            resolve_reasoning_level("openai", "gpt-6-luna", None),
            Some("medium".to_string())
        );
        // Only minimal+high style ladders: below-medium wins over the floor.
        // gemini-3.8-flash has [low, medium, high]; a model with only
        // [minimal, high] would resolve to minimal.
        // Lowest supported level when nothing below medium exists.
        assert_eq!(
            resolve_reasoning_level("anthropic", "claude-haiku-4-5", Some("bogus")),
            Some("medium".to_string())
        );
        // Non-reasoning model: no effort at all.
        assert_eq!(
            resolve_reasoning_level("openai", "gpt-4o", Some("low")),
            None
        );
        // Unknown model: no effort.
        assert_eq!(resolve_reasoning_level("openai", "nope", Some("low")), None);
    }

    #[test]
    fn lowest_reasoning_level_returns_the_cheapest_supported() {
        assert_eq!(
            lowest_reasoning_level("openai", "gpt-6-luna"),
            Some("none".to_string())
        );
        assert_eq!(
            lowest_reasoning_level("anthropic", "claude-haiku-5-5"),
            Some("none".to_string())
        );
        assert_eq!(
            lowest_reasoning_level("gemini", "gemini-3.5-flash-lite"),
            Some("minimal".to_string())
        );
        assert_eq!(lowest_reasoning_level("openai", "gpt-4o"), None);
    }

    #[test]
    fn default_model_is_first_recommended_entry() {
        assert_eq!(default_model_id("openai"), Some("gpt-6-luna"));
        assert_eq!(default_model_id("anthropic"), Some("claude-haiku-5-5"));
        assert_eq!(default_model_id("gemini"), Some("gemini-3.5-flash-lite"));
    }
}
