use super::{catalog, error::AiProviderError};
use tauri_plugin_store::StoreExt;

/// Whether a selected (provider, model) pair satisfies the executor's model
/// requirement. Agent-CLI runtimes (Claude Code) waive it — they carry no
/// catalog model because the CLI selects its own; every other runtime requires
/// a non-empty model. Shared by the readiness/selection guards below so the
/// model-less-CLI exemption lives in exactly one place.
pub(crate) fn selection_meets_model_requirement(provider: &str, model: &str) -> bool {
    !model.is_empty() || catalog::runtime_kind(provider) == Some("agent_cli")
}

pub(crate) fn selected_ai_provider_and_model<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(String, String), AiProviderError> {
    let store = app
        .store("settings")
        .map_err(|_| AiProviderError::Internal)?;
    let provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let model = store
        .get("ai_model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    if provider.is_empty() || !selection_meets_model_requirement(&provider, &model) {
        return Err(AiProviderError::InvalidModel);
    }
    Ok((provider, model))
}
