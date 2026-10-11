//! Recording-owned CLI warmup. No transcript is retained or sent during warmup.
use std::future::Future;
use std::sync::{LazyLock, Mutex};

use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tokio_util::sync::CancellationToken;

use crate::ai::agent_cli::{hot_spare_generation, invalidate_hot_spare, warm_hot_spare_if_current};
use crate::ai::contract::AiPolishRequest;
use crate::ai::providers::PROVIDER_CLAUDE_CODE;

struct Recording {
    generation: u64,
    cancellation: CancellationToken,
    prefetch_cancellation: CancellationToken,
    last_request: Option<(u64, AiPolishRequest)>,
}

static RECORDING: LazyLock<Mutex<Option<Recording>>> = LazyLock::new(|| Mutex::new(None));
tokio::task_local! {
    static POLISH_GENERATION: (u64, u64);
}

pub(crate) fn begin_recording(generation: u64) {
    if let Ok(mut recording) = RECORDING.lock() {
        if let Some(previous) = recording.take() {
            previous.cancellation.cancel();
            previous.prefetch_cancellation.cancel();
        }
        *recording = Some(Recording {
            generation,
            cancellation: CancellationToken::new(),
            prefetch_cancellation: CancellationToken::new(),
            last_request: None,
        });
    }
}

pub(crate) fn cancel_recording() {
    if let Ok(mut recording) = RECORDING.lock() {
        if let Some(recording) = recording.take() {
            recording.cancellation.cancel();
            recording.prefetch_cancellation.cancel();
        }
    }
    invalidate_hot_spare();
}

pub(crate) fn invalidate() {
    // Serialize against remembering a request and queuing replacement warmup.
    if let Ok(mut recording) = RECORDING.lock() {
        invalidate_hot_spare();
        if let Some(recording) = recording.as_mut() {
            recording.last_request = None;
            recording.prefetch_cancellation.cancel();
        }
    } else {
        invalidate_hot_spare();
    }
}

pub(crate) async fn recording_polish<F: Future>(generation: u64, future: F) -> F::Output {
    // Once formatting starts, initial warmup must not compete with polish or
    // delivery. A spare already installed remains available for consumption.
    if let Ok(recording) = RECORDING.lock() {
        if let Some(recording) = recording.as_ref().filter(|r| r.generation == generation) {
            recording.prefetch_cancellation.cancel();
        }
    }
    POLISH_GENERATION
        .scope((generation, hot_spare_generation()), future)
        .await
}

pub(crate) fn polish_cancellation() -> CancellationToken {
    let Ok(generation) = POLISH_GENERATION.try_with(|scope| scope.0) else {
        return CancellationToken::new();
    };
    if let Ok(recording) = RECORDING.lock() {
        if let Some(recording) = recording.as_ref().filter(|r| r.generation == generation) {
            return recording.cancellation.clone();
        }
    }
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    cancelled
}

pub(crate) fn remember_request(request: &AiPolishRequest) {
    if request.provider_id != PROVIDER_CLAUDE_CODE {
        return;
    }
    let Ok((generation, epoch)) = POLISH_GENERATION.try_with(|scope| *scope) else {
        return;
    };
    if let Ok(mut recording) = RECORDING.lock() {
        if let Some(recording) = recording.as_mut().filter(|r| r.generation == generation) {
            if warm_is_current(
                recording.cancellation.is_cancelled(),
                epoch,
                hot_spare_generation(),
            ) {
                // Retain only invocation configuration, never dictated text.
                recording.last_request = Some((
                    epoch,
                    AiPolishRequest {
                        provider_id: request.provider_id.clone(),
                        model_id: request.model_id.clone(),
                        reasoning_level: request.reasoning_level.clone(),
                        fast_mode: request.fast_mode,
                        input_text: String::new(),
                        prompt: request.prompt.clone(),
                        timeout_ms: request.timeout_ms,
                    },
                ));
            }
        }
    }
}

fn eligible(enabled: bool, provider: &str, ready: bool) -> bool {
    enabled && provider == PROVIDER_CLAUDE_CODE && ready
}

fn warm_is_current(cancelled: bool, captured_epoch: u64, current_epoch: u64) -> bool {
    !cancelled && captured_epoch == current_epoch
}

fn current_configuration(app: &AppHandle, request: &AiPolishRequest) -> bool {
    let Ok(store) = app.store("settings") else {
        return false;
    };
    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    enabled && provider == request.provider_id && provider == PROVIDER_CLAUDE_CODE
}

async fn warm(
    app: AppHandle,
    request: AiPolishRequest,
    epoch: u64,
    cancellation: CancellationToken,
) {
    if !warm_is_current(cancellation.is_cancelled(), epoch, hot_spare_generation())
        || !current_configuration(&app, &request)
    {
        return;
    }
    // The runtime resolves a safe executable and validates its streaming flags
    // before spawning. An unavailable CLI is a best-effort no-op.
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => {},
        _ = warm_hot_spare_if_current(&request, epoch) => {},
    }
}

pub(crate) fn delivered(app: AppHandle, generation: u64) {
    let Ok(mut recording) = RECORDING.lock() else {
        return;
    };
    let Some(recording) = recording.as_mut().filter(|r| r.generation == generation) else {
        return;
    };
    if recording.cancellation.is_cancelled() {
        return;
    }
    let Some((epoch, request)) = recording.last_request.take() else {
        return;
    };
    let cancellation = recording.cancellation.clone();
    tokio::spawn(warm(app, request, epoch, cancellation));
}

pub(crate) async fn prefetch(app: AppHandle) {
    // Snapshot the runtime barrier before reading settings or awaiting anything.
    let epoch = hot_spare_generation();
    let (generation, cancellation) = {
        let Ok(recording) = RECORDING.lock() else {
            return;
        };
        let Some(recording) = recording.as_ref() else {
            return;
        };
        (
            recording.generation,
            recording.prefetch_cancellation.clone(),
        )
    };
    let Ok(mut ai) = super::ai::get_ai_settings(app.clone()).await else {
        return;
    };
    if !ai.enabled || ai.provider != PROVIDER_CLAUDE_CODE {
        return;
    }
    let ready = crate::ai::agent_cli::probe(&ai.provider, false).await.state
        == crate::ai::agent_cli::AgentCliProbeState::Ready;
    if !eligible(ai.enabled, &ai.provider, ready) {
        return;
    }
    let Ok(settings) = super::settings::get_settings(app.clone()).await else {
        return;
    };
    let Ok(config) = crate::writing::effective_pipeline_config(&app) else {
        return;
    };
    let Ok(writing) = crate::writing::load_writing_settings(&app) else {
        return;
    };
    let transcript_language = if settings.transcription_task
        == super::settings::TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH
    {
        Some("en")
    } else if settings.speech_language == "auto" {
        None
    } else {
        Some(settings.speech_language.as_str())
    };
    let output_language =
        if config.final_text_language == super::settings::FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT {
            transcript_language.unwrap_or(super::settings::FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT)
        } else {
            config.final_text_language.as_str()
        };
    let context = crate::writing::smart_formatting_ai_context(&writing, transcript_language);
    let category = config.category_hint.and_then(|category| {
        crate::writing::category_prompt_hint(category).map(|hint| {
            format!(
                "You are dictating into a {} context. {}",
                crate::writing::category_label(category),
                hint
            )
        })
    });
    let prompt = crate::ai::prompts::build_enhancement_prompt_for_transcript_language(
        context.as_deref(),
        &crate::ai::EnhancementOptions {
            preset: config.preset,
        },
        Some(output_language),
        transcript_language,
        category.as_deref(),
    );
    let reasoning = ai
        .reasoning_by_provider
        .remove(PROVIDER_CLAUDE_CODE)
        .unwrap_or_else(|| "medium".to_string());
    let request = AiPolishRequest {
        provider_id: ai.provider,
        model_id: ai.model,
        reasoning_level: Some(reasoning),
        fast_mode: ai
            .fast_mode_by_provider
            .remove(PROVIDER_CLAUDE_CODE)
            .unwrap_or(false),
        input_text: String::new(),
        prompt,
        timeout_ms: crate::ai::agent_cli::cold_spawn_timeout_ms(PROVIDER_CLAUDE_CODE),
    };
    if super::audio::recording_generation_is_stale(generation) {
        return;
    }
    warm(app, request, epoch, cancellation).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn warm_eligibility_requires_enabled_selected_ready_claude() {
        assert!(eligible(true, PROVIDER_CLAUDE_CODE, true));
        assert!(!eligible(false, PROVIDER_CLAUDE_CODE, true));
        assert!(!eligible(true, PROVIDER_CLAUDE_CODE, false));
        for provider in ["", "openai", "codex", "pi"] {
            assert!(!eligible(true, provider, true));
        }
    }

    #[test]
    fn cancellation_and_settings_barrier_reject_pending_warm() {
        assert!(warm_is_current(false, 7, 7));
        assert!(!warm_is_current(true, 7, 7));
        assert!(!warm_is_current(false, 7, 8));
        assert!(!warm_is_current(true, 7, 8));
    }
}
