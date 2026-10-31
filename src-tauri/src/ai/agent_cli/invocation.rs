//! Request orchestration, including one-shot warm consumption and cold fallback.
use super::*;

/// Each invocation consumes a fresh child; a Claude spare is never reused.
#[derive(Clone, Default)]
pub struct AgentCliRuntime;

impl AgentCliRuntime {
    pub fn new() -> Self {
        Self
    }

    /// Polish `request.input_text` under `request.prompt`, returning the RAW
    /// model string. The executor outer loop (executor.rs) sanitizes, validates,
    /// and retries — this method does NOT duplicate that. Any spawn/timeout/parse
    /// failure returns `Err`, which the executor surfaces as `ai_error` → the
    /// existing raw-transcript fallback delivers deterministic text + toast.
    #[cfg(test)]
    pub async fn polish(&self, request: &AiPolishRequest) -> Result<String, MappedAiProviderError> {
        self.polish_with_budget(request, Duration::from_millis(request.timeout_ms))
            .await
    }

    pub(crate) async fn polish_with_budget(
        &self,
        request: &AiPolishRequest,
        budget: Duration,
    ) -> Result<String, MappedAiProviderError> {
        let deadline = Instant::now() + budget;
        let spec = spec_for(&request.provider_id)
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;

        // pi/omp own their catalogs; never pass a removed saved selector or
        // delegate to an unavailable CLI default. Discovery also lets the
        // chooser offer valid models when the external default has gone stale.
        if matches!(request.provider_id.as_str(), PROVIDER_PI | PROVIDER_OMP) {
            let models = list_models(&request.provider_id).await?;
            if let Err(error) = validate_discovered_selection(&models, &request.model_id) {
                model_cache::invalidate(&request.provider_id);
                return Err(error);
            }
        }

        let binary_path = resolve_binary(spec.binary)
            .await
            .ok_or_else(|| MappedAiProviderError::new(AiProviderError::UnsupportedProvider))?;
        let capability_started = Instant::now();
        let (claude_capabilities, help, cached_capabilities) =
            discover_capabilities_for_polish(&binary_path, spec, false).await;
        log::info!(
            "agent_cli_stage_timing provider={} stage=capability_probe duration_ms={} cached={}",
            spec.provider_id,
            capability_started.elapsed().as_millis(),
            cached_capabilities
        );
        let Some(spec) = compatible_spec(spec, &help) else {
            return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
                "The installed CLI does not support the required isolation flags.".to_string(),
            )));
        };

        let prompt = request.prompt.clone();
        let input_text = if spec.system_prompt == SystemPromptPolicy::None {
            compose_cli_input(&prompt, &request.input_text)
        } else {
            request.input_text.clone()
        };
        let input_mode = spec.input_mode;
        let output = spec.output;
        let mut argv = cold_argv_for_model_with_options(
            spec,
            &prompt,
            &request.model_id,
            claude_capabilities,
            request.reasoning_level.as_deref(),
            request.fast_mode,
        );
        let variant_suffix = matches!(spec.reasoning, ReasoningPolicy::ModelVariantSuffix { .. });
        if variant_suffix && model_variant_rejected_before(&argv) {
            strip_model_variant(&mut argv);
        }
        let invocation_started = Instant::now();
        // At most half of the remaining outer budget is spent on the warm
        // attempt, leaving the rest for a cold process if it fails.
        let remaining = deadline.saturating_duration_since(Instant::now());
        let hot_result = if spec.provider_id == PROVIDER_CLAUDE_CODE {
            CLAUDE_SPARE
                .take_and_run_with_budget(&binary_path, &argv, &input_text, remaining / 2)
                .await
        } else {
            invalidate_hot_spare();
            None
        };
        let mut result = match hot_result {
            Some(Ok(text)) => Ok(text),
            Some(Err(_)) | None => {
                cold_spawn_and_collect(spec, &binary_path, &argv, input_mode, &input_text, output)
                    .await
            }
        };
        if variant_suffix && result.as_ref().is_err_and(is_variant_unavailable) {
            if let Some(selector) = strip_model_variant(&mut argv) {
                log::warn!(
                    "OpenCode has no reasoning variant {selector}; retrying once without it"
                );
                rejected_model_variants().insert(selector);
                result = cold_spawn_and_collect(
                    spec,
                    &binary_path,
                    &argv,
                    input_mode,
                    &input_text,
                    output,
                )
                .await;
            }
        }
        if result
            .as_ref()
            .is_err_and(model_cache::is_model_or_auth_failure)
        {
            model_cache::invalidate(&request.provider_id);
        }
        log::info!(
            "agent_cli_stage_timing provider={} stage=model_invocation duration_ms={} outcome={}",
            spec.provider_id,
            invocation_started.elapsed().as_millis(),
            if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            }
        );
        result
    }
}
