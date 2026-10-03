//! Shared, state-free prompt/executor boundary used by desktop and measurement CLI.
use super::{
    contract::{AiPolishRequest, AiPolishResult},
    error::AiProviderError,
    executor::AiExecutor,
    EnhancementOptions,
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone)]
pub(crate) struct PolishRuntime {
    pub executor: AiExecutor,
    pub provider: String,
    pub model: String,
    pub reasoning_level: Option<String>,
    pub fast_mode: bool,
    pub timeout_ms: u64,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolishTimings {
    pub prompt: f64,
    pub request: f64,
    pub validate: f64,
    pub total: f64,
}

pub(crate) fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

pub(crate) fn assemble_prompt(
    options: &EnhancementOptions,
    output_language: Option<&str>,
    transcript_language: Option<&str>,
    context: Option<&str>,
    app_category_hint: Option<&str>,
    keep_words: bool,
) -> String {
    super::prompts::build_prompt_with_wording(
        context,
        options,
        output_language,
        transcript_language,
        app_category_hint,
        keep_words,
    )
}

pub(crate) async fn execute_prompt(
    runtime: &PolishRuntime,
    text: &str,
    prompt: String,
    timings: &mut PolishTimings,
    needs_output_language_transform: bool,
) -> Result<AiPolishResult, AiProviderError> {
    runtime
        .executor
        .polish_with_timings(
            AiPolishRequest {
                provider_id: runtime.provider.clone(),
                model_id: runtime.model.clone(),
                reasoning_level: runtime.reasoning_level.clone(),
                fast_mode: runtime.fast_mode,
                input_text: text.to_string(),
                needs_output_language_transform,
                prompt,
                timeout_ms: runtime.timeout_ms,
            },
            tokio_util::sync::CancellationToken::new(),
            timings,
        )
        .await
}

// The writing pipeline's existing raw fallback, also used by measurement results.
pub(crate) fn raw_fallback(text: &str) -> String {
    text.to_string()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolishOutcome {
    Polished,
    Skipped,
    FallbackRaw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolishOutput {
    pub output: String,
    pub outcome: PolishOutcome,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fallback_reason: Option<String>,
    pub provider: String,
    pub model: String,
    pub timings_ms: PolishTimings,
}

pub(crate) async fn measure(
    runtime: &PolishRuntime,
    text: &str,
    options: &EnhancementOptions,
    language: Option<&str>,
    context: Option<&str>,
    category_hint: Option<&str>,
    keep_words: bool,
) -> PolishOutput {
    let start = Instant::now();
    let mut timings = PolishTimings::default();
    let (output, outcome, fallback_reason) = if !options.preset.requires_ai_formatting()
        || super::skip::should_skip(text, options.preset)
    {
        (text.to_string(), PolishOutcome::Skipped, None)
    } else {
        let prompt_start = Instant::now();
        let prompt = assemble_prompt(
            options,
            language,
            language,
            context,
            category_hint,
            keep_words,
        );
        timings.prompt = elapsed_ms(prompt_start);
        match execute_prompt(runtime, text, prompt, &mut timings, false).await {
            Ok(result) => (result.output_text, PolishOutcome::Polished, None),
            Err(error) => (
                raw_fallback(text),
                PolishOutcome::FallbackRaw,
                Some(error_category(&error).to_string()),
            ),
        }
    };
    timings.total = elapsed_ms(start);
    PolishOutput {
        output,
        outcome,
        fallback_reason,
        provider: runtime.provider.clone(),
        model: runtime.model.clone(),
        timings_ms: timings,
    }
}

// Agent errors may contain arbitrary subprocess text. Never serialize that text.
pub(crate) fn error_category(error: &AiProviderError) -> &'static str {
    match error {
        AiProviderError::MissingApiKey => "missing_api_key",
        AiProviderError::InvalidApiKey => "invalid_api_key",
        AiProviderError::InvalidModel => "invalid_model",
        AiProviderError::UnsupportedProvider => "unsupported_provider",
        AiProviderError::Timeout => "timeout",
        AiProviderError::Canceled => "canceled",
        AiProviderError::RateLimited => "rate_limited",
        AiProviderError::ServiceUnavailable => "service_unavailable",
        AiProviderError::Network => "network",
        AiProviderError::BadResponse => "bad_response",
        AiProviderError::Internal => "internal",
        AiProviderError::OutputGuard(reason) => reason.code(),
        AiProviderError::AgentCli(_) => "agent_cli",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn offline_runtime() -> PolishRuntime {
        PolishRuntime {
            executor: AiExecutor::new(
                reqwest::Client::new(),
                std::sync::Arc::new(|_| None),
                String::new(),
                true,
            ),
            provider: "unsupported-test-provider".to_string(),
            model: "test-model".to_string(),
            reasoning_level: None,
            fast_mode: false,
            timeout_ms: 100,
        }
    }
    #[tokio::test]
    async fn off_is_byte_identical_and_provider_failure_falls_back_without_network() {
        let runtime = offline_runtime();
        let text = "  What about request_id?\n";
        let off = measure(
            &runtime,
            text,
            &EnhancementOptions {
                preset: super::super::prompts::EnhancementPreset::PersonalDictation,
            },
            None,
            None,
            None,
            false,
        )
        .await;
        assert!(off.output == text, "off changed the input");
        assert_eq!(off.outcome, PolishOutcome::Skipped);
        assert_eq!(off.timings_ms.request, 0.0);
        let failed = measure(
            &runtime,
            text,
            &EnhancementOptions {
                preset: super::super::prompts::EnhancementPreset::CleanDictation,
            },
            Some("en"),
            None,
            None,
            false,
        )
        .await;
        assert!(
            failed.output == text,
            "provider failure changed raw fallback"
        );
        assert_eq!(failed.outcome, PolishOutcome::FallbackRaw);
        assert_eq!(
            failed.fallback_reason.as_deref(),
            Some("unsupported_provider")
        );
        assert!(
            failed.timings_ms.total
                >= failed.timings_ms.prompt
                    + failed.timings_ms.request
                    + failed.timings_ms.validate
        );
        assert_eq!(
            error_category(&AiProviderError::AgentCli("private payload".to_string())),
            "agent_cli"
        );
    }
    #[tokio::test]
    async fn zero_wait_never_calls_provider() {
        for keep_words in [false, true] {
            let result = measure(
                &offline_runtime(),
                "Ready.",
                &EnhancementOptions {
                    preset: super::super::prompts::EnhancementPreset::CleanDictation,
                },
                Some("en"),
                None,
                None,
                keep_words,
            )
            .await;
            assert_eq!(result.outcome, PolishOutcome::Skipped);
            assert_eq!(result.timings_ms.request, 0.0);
            assert_eq!(result.timings_ms.prompt, 0.0);
            assert!(result.output == "Ready.");
        }
    }
    #[test]
    fn prompt_assembly_delegates_without_changing_language_context_or_style() {
        let options = EnhancementOptions {
            preset: super::super::prompts::EnhancementPreset::Notes,
        };
        assert!(
            assemble_prompt(
                &options,
                Some("fr"),
                Some("en"),
                Some("Zorvi"),
                Some("notes hint"),
                false
            ) == super::super::prompts::build_enhancement_prompt_for_transcript_language(
                Some("Zorvi"),
                &options,
                Some("fr"),
                Some("en"),
                Some("notes hint")
            )
        );
    }
}
