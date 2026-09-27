//! Stage 1 transcription executor.
//!
//! `transcribe_with_app` is the single entry point that resolves an
//! [`EngineSelection`] to a concrete engine and runs the job, returning the
//! existing [`TranscriptionResult`] shape or a typed [`TranscriptionError`].
//!
//! Stage 1 (plan 014) is additive: no existing callsite is rewired yet. The
//! executor fully handles `EngineSelection::Explicit` routing to the local and
//! cloud engines (Whisper / Parakeet / Cloud) by delegating to the existing
//! engine helpers. Two routes are intentionally deferred to later migration
//! stages and return a typed error if reached:
//! - `EngineSelection::HostDefault` (remote-server inbound) — Stage 4.
//! - `EngineSelection::Explicit { engine: Remote, .. }` (send-to-peer) — Stage 5 (multipart wire).
//!
//! `run_with_policy` enforces [`TimeoutPolicy`] at this seam (plan 015): an
//! interactive watchdog sets the shared cancellation flag on deadline (it never
//! aborts the blocking decode), Whisper retries, and cloud uses a network timeout.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tempfile::{NamedTempFile, TempPath};

pub(crate) const LOCAL_ENGINE_TIMEOUT_GRACE: Duration = Duration::from_secs(2);

use crate::parakeet::manager::{ParakeetManager, ParakeetTranscriptionOptions};
use crate::parakeet::messages::ParakeetResponse;
use crate::provider_capabilities::ProviderEngine;
use crate::secure_store::secure_get;
use crate::transcription::engines::{
    parakeet_segments_to_transcription_segments, resolve_engine_for_model,
    transcribe_whisper_with_acceleration, transcription_watchdog_budget, ActiveEngineSelection,
};
use crate::transcription::error::{
    from_local_engine_string, from_stt_error, TranscriptionError, TranscriptionErrorCode,
};
use crate::transcription::request::{
    CancellationToken, CleanupPolicy, EngineSelection, TimeoutPolicy, TranscriptionAudio,
    TranscriptionRequest,
};
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSource, TranscriptionTask,
};

/// Resolve, route, and run one transcription request.
pub async fn transcribe_with_app(
    app: &AppHandle,
    request: TranscriptionRequest,
) -> Result<TranscriptionResult, TranscriptionError> {
    let active = resolve_active(app, &request.engine, request.source).await?;
    transcribe_with_resolved_engine(app, request, active).await
}

/// Keep the normal executor policy and result handling for a CLI supplied Whisper file.
pub(crate) async fn transcribe_with_resolved_engine(
    app: &AppHandle,
    request: TranscriptionRequest,
    active: ActiveEngineSelection,
) -> Result<TranscriptionResult, TranscriptionError> {
    let source = request.source;
    let job = TranscriptionJob {
        source,
        engine: active.engine_name().to_string(),
        model: active.model_name().to_string(),
        spoken_language: request.spoken_language.clone(),
        task: request.task,
    };

    // Materialize audio to a filesystem path. Inline bytes are staged into an
    // owned temp file that is deleted when `_bytes_temp` drops.
    let (input_path, _bytes_temp, caller_input) = match &request.audio {
        TranscriptionAudio::Path { path, cleanup, .. } => {
            let caller = match cleanup {
                CleanupPolicy::CallerOwns => None,
                other => Some((path.clone(), other.clone())),
            };
            (path.clone(), None, caller)
        }
        TranscriptionAudio::Bytes { bytes, .. } => {
            let temp = NamedTempFile::new().map_err(|e| stage_error(source, "temp create", e))?;
            std::fs::write(temp.path(), bytes).map_err(|e| stage_error(source, "temp write", e))?;
            (temp.path().to_path_buf(), Some(temp), None)
        }
    };

    let outcome = run_with_policy(app, &request, &active, &job, &input_path).await;

    // Apply the caller's cleanup policy to a caller-provided Path input.
    if let Some((path, policy)) = caller_input {
        let (success, retryable) = match &outcome {
            Ok(_) => (true, false),
            Err(e) => (false, e.retryable),
        };
        if should_delete_input(&policy, success, retryable) {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!(
                    "executor: failed to remove input audio {}: {e}",
                    path.display()
                );
            }
        }
    }

    outcome
}

async fn resolve_active(
    app: &AppHandle,
    engine: &EngineSelection,
    source: TranscriptionSource,
) -> Result<ActiveEngineSelection, TranscriptionError> {
    if let Some(error) = deferred_executor_route_error(engine, source) {
        return Err(error);
    }

    match engine {
        EngineSelection::Explicit { engine, model } => {
            resolve_engine_for_model(app, model, Some(engine.as_str()))
                .await
                .map_err(|e| from_local_engine_string(&e, source))
        }
        EngineSelection::HostDefault => unreachable!("deferred route checked before resolution"),
    }
}

fn deferred_executor_route_error(
    engine: &EngineSelection,
    source: TranscriptionSource,
) -> Option<TranscriptionError> {
    match engine {
        EngineSelection::Explicit {
            engine: ProviderEngine::Remote,
            ..
        } => Some(TranscriptionError::new(
            TranscriptionErrorCode::Internal,
            source,
            "Remote engine selection is handled outside the transcription executor by the desktop remote transcription path.",
        )),
        // Stage 4: remote-server inbound snapshots its own shared engine/model.
        EngineSelection::HostDefault => Some(TranscriptionError::new(
            TranscriptionErrorCode::Internal,
            source,
            "Host-default engine resolution is wired by the remote-server inbound port (Stage 4).",
        )),
        _ => None,
    }
}

/// Reject a translate-to-English task a cloud engine cannot honor.
///
/// Every curated cloud provider transcribes in the spoken language
/// (`supports_translate_task == false`). Routing such a request through would
/// return source-language text that `TranscriptionResult::new` then mislabels
/// as English — it falls back to `"en"` for a translate task. Reject up front
/// so the caller never receives mislabeled output. An engine that genuinely
/// supports the task passes through unchanged (none of the cloud engines do
/// today).
pub(crate) fn ensure_cloud_task_supported(
    provider: crate::cloud_stt::CloudProvider,
    translate: bool,
    source: TranscriptionSource,
) -> Result<(), TranscriptionError> {
    if translate
        && !ProviderEngine::from_engine_str(provider.id())
            .map(|engine| engine.capabilities().supports_translate_task)
            .unwrap_or(false)
    {
        return Err(TranscriptionError::new(
            TranscriptionErrorCode::EngineUnavailable,
            source,
            format!(
                "{} cannot translate to English. Choose a local engine that can, such as Whisper.",
                provider.display_name()
            ),
        ));
    }
    Ok(())
}

async fn route_once(
    app: &AppHandle,
    active: &ActiveEngineSelection,
    job: &TranscriptionJob,
    input_path: &Path,
    request: &TranscriptionRequest,
) -> Result<TranscriptionResult, TranscriptionError> {
    let source = request.source;
    let language = request.spoken_language.as_deref();
    let translate = matches!(request.task, TranscriptionTask::TranslateToEnglish);

    match active {
        ActiveEngineSelection::Whisper { model_path, .. } => {
            let token = request.cancellation.clone();
            let output = transcribe_whisper_with_acceleration(
                app,
                model_path,
                input_path,
                language,
                translate,
                request.initial_prompt.as_deref(),
                request.audio_ctx,
                request.speed_mode_override,
                move || token.is_cancelled(),
            )
            .await
            .map_err(|e| from_local_engine_string(&e, source))?;

            let result = TranscriptionResult::new(job, output.raw_text)
                .with_transcript_language(output.transcript_language)
                .with_segments(output.segments)
                .with_audio_duration_ms(Some(output.audio_duration_ms))
                .with_processing_duration_ms(Some(output.processing_duration_ms));
            let result = if matches!(source, TranscriptionSource::AudioFile) {
                result.with_span_timings_ms(serde_json::json!({
                    "preprocessing": output.timings.preprocessing_ms,
                    "inference": output.timings.inference_ms,
                    "extraction": output.timings.extraction_ms,
                    "total": output.timings.total_ms,
                }))
            } else {
                result
            };
            Ok(result)
        }
        ActiveEngineSelection::Parakeet { model_name } => {
            let manager = app.state::<ParakeetManager>();
            let cancel = request.cancellation.as_arc();

            if let Err(e) = manager
                .load_model_with_cancel(app, model_name, Some(cancel.clone()))
                .await
            {
                if request.cancellation.is_cancelled() {
                    return Err(cancelled(source));
                }
                return Err(from_local_engine_string(
                    &format!("Parakeet model load failed: {e}"),
                    source,
                ));
            }
            if request.cancellation.is_cancelled() {
                return Err(cancelled(source));
            }

            let custom_vocabulary = if model_name.starts_with("parakeet-tdt-") {
                crate::writing::load_writing_settings(app)
                    .map(|settings| crate::writing::compile_parakeet_custom_vocabulary(
                        &settings,
                        request.spoken_language.as_deref(),
                    ))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            match manager
                .transcribe_with_custom_vocabulary(
                    app,
                    model_name,
                    input_path.to_path_buf(),
                    ParakeetTranscriptionOptions {
                        language: request.spoken_language.clone(),
                        translate,
                        custom_vocabulary,
                        cancel_flag: Some(cancel),
                    },
                )
                .await
            {
                Ok(ParakeetResponse::Transcription {
                    text,
                    segments,
                    language,
                    duration,
                }) => {
                    let timings = manager.latest_timing_snapshot();
                    let result = TranscriptionResult::new(job, text)
                        .with_transcript_language(language)
                        .with_segments(parakeet_segments_to_transcription_segments(segments))
                        .with_audio_duration_ms(effective_parakeet_audio_duration_ms(
                            duration, input_path,
                        ));
                    let result = if matches!(source, TranscriptionSource::AudioFile) {
                        result.with_span_timings_ms(serde_json::json!({
                            "model_load": timings.model_load_ms,
                            "inference": timings.inference_ms,
                            "total": timings.total_ms,
                        }))
                    } else {
                        result
                    };
                    Ok(result)
                }
                Ok(ParakeetResponse::Error { code, message, .. }) => Err(TranscriptionError::new(
                    TranscriptionErrorCode::EngineFailed,
                    source,
                    "Transcription failed",
                )
                .with_detail(format!("Parakeet error {code}: {message}"))),
                Ok(_) => Err(TranscriptionError::new(
                    TranscriptionErrorCode::ResponseInvalid,
                    source,
                    "Transcription failed",
                )
                .with_detail("unexpected Parakeet response")),
                Err(e) => Err(from_local_engine_string(
                    &format!("Parakeet transcription failed: {e}"),
                    source,
                )),
            }
        }
        ActiveEngineSelection::Cloud { provider, .. } => {
            ensure_cloud_task_supported(*provider, translate, source)?;

            let key = match secure_get(app, provider.key_name()) {
                Ok(Some(key)) if !key.trim().is_empty() => key,
                _ => {
                    return Err(TranscriptionError::new(
                        TranscriptionErrorCode::Unauthorized,
                        source,
                        format!("{} API key not set", provider.display_name()),
                    ))
                }
            };
            let model = provider.selected_model(app).id;
            match provider.transcribe_typed(app, &key, model, input_path, language).await {
                Ok(text) => Ok(TranscriptionResult::new(job, text)),
                Err(e) => Err(from_stt_error(&e, source)),
            }
        }
        // Stage 5: sending to a peer goes through the multipart remote client.
        ActiveEngineSelection::Remote { .. } => Err(TranscriptionError::new(
            TranscriptionErrorCode::Internal,
            source,
            "Remote (send-to-peer) transcription via the executor is wired by the remote-client port (Stage 5).",
        )),
    }
}

/// Run one engine attempt under the request's [`TimeoutPolicy`], carrying the
/// desktop hot-path behaviors plan 015 enforces: a single normalization for local
/// engines, an interactive watchdog that sets the shared cancellation flag on
/// deadline (NEVER aborting the blocking decode), Whisper retry, and a network
/// timeout for cloud.
async fn run_with_policy(
    app: &AppHandle,
    request: &TranscriptionRequest,
    active: &ActiveEngineSelection,
    job: &TranscriptionJob,
    input_path: &Path,
) -> Result<TranscriptionResult, TranscriptionError> {
    let source = request.source;

    // Prepare the per-engine attempt input ONCE, before any retry: local engines
    // need a 16 kHz mono WAV; cloud/remote take the input path as-is.
    let prepared = match active {
        ActiveEngineSelection::Whisper { .. } | ActiveEngineSelection::Parakeet { .. } => {
            Some(prepare_normalized_input(input_path, source).await?)
        }
        ActiveEngineSelection::Cloud { .. } | ActiveEngineSelection::Remote { .. } => None,
    };
    let attempt_path = prepared
        .as_ref()
        .map(PreparedInput::path)
        .unwrap_or(input_path);

    let budget = watchdog_budget_for(attempt_path, &request.timeout);

    match active {
        // Cloud is network IO: an async timeout that drops the in-flight future is
        // safe and is the only way to bound a request the provider will not cancel.
        ActiveEngineSelection::Cloud { .. } => match budget {
            Some(deadline) => match tokio::time::timeout(
                deadline,
                route_once(app, active, job, attempt_path, request),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(timed_out_error(source)),
            },
            None => route_once(app, active, job, attempt_path, request).await,
        },
        // Local engines run a BLOCKING decode: a sibling watchdog flips the shared
        // cancel flag on deadline; the engine observes it and aborts cooperatively.
        // (Remote via the executor is deferred to Stage 5 and returns immediately.)
        _ => {
            let timed_out = Arc::new(AtomicBool::new(false));
            let watchdog = budget.map(|deadline| {
                spawn_cancel_watchdog(
                    app,
                    request.cancellation.clone(),
                    deadline,
                    timed_out.clone(),
                )
            });

            // Run the local engine. The helper returns the RAW result for any
            // decode that completed within the hard deadline — it never consults
            // the watchdog flag on that path — so we deliberately do NOT remap
            // here. Reading `timed_out` while the watchdog is still live races a
            // decode that completed near the cooperative deadline (the watchdog
            // could set the flag in the gap between engine completion and the
            // read) and would convert a genuine success into a Timeout.
            let result = if matches!(active, ActiveEngineSelection::Whisper { .. }) {
                run_local_engine_with_hard_timeout(
                    run_whisper_with_retry(app, active, job, attempt_path, request),
                    budget,
                    LOCAL_ENGINE_TIMEOUT_GRACE,
                    request.cancellation.clone(),
                    timed_out.clone(),
                    source,
                )
                .await
            } else {
                route_once(app, active, job, attempt_path, request).await
            };

            // Abort the cooperative watchdog BEFORE consulting `timed_out`. Once
            // aborted it can no longer set the flag, closing the completion-window
            // race for a decode that beat the cooperative deadline.
            if let Some(handle) = watchdog {
                handle.abort();
            }

            // The helper's hard-timeout branch already returned a Timeout; don't
            // rebuild it. Remap only non-timeout results when the watchdog fired
            // — its firing makes every outcome untrusted (cancellation requested,
            // and on Windows the GPU sidecar may have been aborted mid-decode).
            if matches!(
                &result,
                Err(error) if error.code == TranscriptionErrorCode::Timeout
            ) {
                return result;
            }

            remap_timed_out(result, timed_out.load(Ordering::SeqCst), source)
        }
    }
}

/// A 16 kHz mono WAV ready for a local engine: either a temp we normalized into
/// (deleted on drop) or a borrow of a caller input that already conforms.
enum PreparedInput {
    // A `TempPath` (not a live `NamedTempFile`): the decoder atomically renames its
    // result onto this path, and on Windows you cannot rename over an *open* file. We
    // keep only the path (handle closed) so the replace succeeds, while still deleting
    // the normalized WAV on drop.
    Owned(TempPath),
    AlreadyNormalized(PathBuf),
}

impl PreparedInput {
    fn path(&self) -> &Path {
        match self {
            PreparedInput::Owned(temp) => temp.as_ref(),
            PreparedInput::AlreadyNormalized(path) => path.as_path(),
        }
    }
}

/// Prepare a 16 kHz mono WAV for a local engine, skipping the (CPU-bound) decode
/// step when the input already conforms (e.g. the desktop pre-normalizes).
async fn prepare_normalized_input(
    input_path: &Path,
    source: TranscriptionSource,
) -> Result<PreparedInput, TranscriptionError> {
    if is_normalized_wav(input_path) {
        return Ok(PreparedInput::AlreadyNormalized(input_path.to_path_buf()));
    }
    // Reserve a temp path but release the file handle: the decoder finalizes by
    // renaming onto this path, and Windows refuses to replace a still-open file.
    let out = NamedTempFile::new()
        .map_err(|e| stage_error(source, "temp create", e))?
        .into_temp_path();
    crate::audio::decode::normalize_to_wav_async(input_path.to_path_buf(), out.to_path_buf())
        .await
        .map_err(|e| from_local_engine_string(&e, source))?;
    Ok(PreparedInput::Owned(out))
}

/// True if `path` is already a 16 kHz, mono, 16-bit-int WAV — the engine input
/// contract — so re-normalizing it would be a wasteful no-op.
fn is_normalized_wav(path: &Path) -> bool {
    match hound::WavReader::open(path) {
        Ok(reader) => {
            let spec = reader.spec();
            spec.sample_rate == 16_000
                && spec.channels == 1
                && spec.bits_per_sample == 16
                && spec.sample_format == hound::SampleFormat::Int
        }
        Err(_) => false,
    }
}

/// Duration of a WAV file in milliseconds, or `None` if it cannot be read.
fn wav_duration_ms(path: &Path) -> Option<u64> {
    let reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    if spec.sample_rate == 0 || spec.channels == 0 {
        return None;
    }
    let frames = (reader.duration() / spec.channels as u32) as u64;
    Some(
        frames
            .saturating_mul(1000)
            .saturating_add(spec.sample_rate as u64 - 1)
            / spec.sample_rate as u64,
    )
}

/// Returns the audio duration in milliseconds for a Parakeet transcription.
///
/// The Parakeet sidecar's `duration` field is often `0.0` (a known FluidAudio
/// bug). When `sidecar_secs` is `Some(s)` with `s > 0.0` we trust it; otherwise
/// we fall back to reading the WAV header of `input_path` directly.
pub(crate) fn effective_parakeet_audio_duration_ms(
    sidecar_secs: Option<f32>,
    input_path: &std::path::Path,
) -> Option<u64> {
    if let Some(s) = sidecar_secs {
        if s > 0.0 {
            return Some((s * 1000.0) as u64);
        }
    }
    crate::parakeet::messages::wav_duration_seconds(input_path)
        .map(|s| (s.max(0.0) * 1000.0) as u64)
}

/// The watchdog/timeout budget for an attempt, or `None` to run unbounded.
pub(crate) fn watchdog_budget_for(attempt_path: &Path, policy: &TimeoutPolicy) -> Option<Duration> {
    match policy {
        TimeoutPolicy::None => None,
        TimeoutPolicy::Explicit(deadline) => Some(*deadline),
        TimeoutPolicy::Interactive | TimeoutPolicy::Upload => {
            Some(transcription_watchdog_budget(wav_duration_ms(attempt_path)))
        }
    }
}

/// The plan 015 interactive watchdog: on deadline, set `timed_out` and the shared
/// cancellation flag (the blocking engines observe it and abort), and on Windows
/// abort the GPU sidecar. The caller aborts the returned handle if the engine
/// finishes first.
fn spawn_cancel_watchdog(
    app: &AppHandle,
    cancellation: CancellationToken,
    budget: Duration,
    timed_out: Arc<AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    #[cfg(target_os = "windows")]
    let app = app.clone();
    #[cfg(not(target_os = "windows"))]
    let _ = app;
    tokio::spawn(async move {
        tokio::time::sleep(budget).await;
        timed_out.store(true, Ordering::SeqCst);
        cancellation.cancel();
        log::warn!(
            "Transcription watchdog timed out after {} seconds",
            budget.as_secs()
        );
        #[cfg(target_os = "windows")]
        {
            app.state::<crate::whisper::gpu_sidecar::GpuSidecarClient>()
                .abort_active_process()
                .await;
        }
    })
}

async fn run_local_engine_with_hard_timeout<Fut>(
    run: Fut,
    budget: Option<Duration>,
    grace: Duration,
    cancellation: CancellationToken,
    timed_out: Arc<AtomicBool>,
    source: TranscriptionSource,
) -> Result<TranscriptionResult, TranscriptionError>
where
    Fut: std::future::Future<Output = Result<TranscriptionResult, TranscriptionError>>,
{
    let Some(deadline) = budget else {
        // No hard bound: run to completion. The cooperative watchdog (if any) is
        // consulted by the caller AFTER it aborts the watchdog, so return the raw
        // result here rather than remapping against a still-live flag.
        return run.await;
    };

    let hard_deadline = deadline.saturating_add(grace);
    match tokio::time::timeout(hard_deadline, run).await {
        // The engine completed within the hard deadline. Return the raw result;
        // the caller remaps against the (by-then aborted) watchdog flag. Reading
        // `timed_out` here would race the still-live cooperative watchdog and
        // could convert a genuine success that landed near the cooperative
        // deadline into a Timeout.
        Ok(result) => result,
        Err(_) => {
            timed_out.store(true, Ordering::SeqCst);
            cancellation.cancel();
            log::warn!(
                "Transcription hard timeout elapsed after {} seconds",
                hard_deadline.as_secs()
            );
            Err(timed_out_error(source))
        }
    }
}

/// Whisper desktop parity: retry the engine up to 3 times (500 ms apart),
/// breaking immediately on cancellation. Normalization is NOT repeated — the
/// attempt path is prepared once by the caller.
async fn run_whisper_with_retry(
    app: &AppHandle,
    active: &ActiveEngineSelection,
    job: &TranscriptionJob,
    attempt_path: &Path,
    request: &TranscriptionRequest,
) -> Result<TranscriptionResult, TranscriptionError> {
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 500;
    let source = request.source;

    let mut result = route_once(app, active, job, attempt_path, request).await;
    let mut attempt = 1;
    while attempt < MAX_RETRIES {
        match &result {
            Ok(_) => break,
            Err(error) if error.code == TranscriptionErrorCode::Cancelled => break,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                if request.cancellation.is_cancelled() {
                    return Err(cancelled(source));
                }
                attempt += 1;
                result = route_once(app, active, job, attempt_path, request).await;
            }
        }
    }
    result
}

/// When the watchdog fired, every outcome is untrusted: cancellation has been
/// requested and the Windows GPU sidecar may have been aborted. Surface timeout.
fn remap_timed_out(
    result: Result<TranscriptionResult, TranscriptionError>,
    timed_out: bool,
    source: TranscriptionSource,
) -> Result<TranscriptionResult, TranscriptionError> {
    if timed_out {
        Err(timed_out_error(source))
    } else {
        result
    }
}

fn timed_out_error(source: TranscriptionSource) -> TranscriptionError {
    TranscriptionError::new(
        TranscriptionErrorCode::Timeout,
        source,
        "Transcription timed out",
    )
}

fn stage_error(source: TranscriptionSource, what: &str, err: std::io::Error) -> TranscriptionError {
    TranscriptionError::new(
        TranscriptionErrorCode::Internal,
        source,
        "Failed to stage audio for transcription",
    )
    .with_detail(format!("{what}: {err}"))
}

fn cancelled(source: TranscriptionSource) -> TranscriptionError {
    TranscriptionError::new(
        TranscriptionErrorCode::Cancelled,
        source,
        "Transcription cancelled",
    )
}

/// Whether a caller-provided input file should be removed after the attempt.
fn should_delete_input(policy: &CleanupPolicy, success: bool, retryable: bool) -> bool {
    match policy {
        CleanupPolicy::CallerOwns => false,
        CleanupPolicy::DeleteAfterAttempt => true,
        CleanupPolicy::PreserveOnRetryableFailure => success || !retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::should_delete_input;
    use super::{
        deferred_executor_route_error, effective_parakeet_audio_duration_ms,
        ensure_cloud_task_supported, is_normalized_wav, remap_timed_out,
        run_local_engine_with_hard_timeout, watchdog_budget_for, wav_duration_ms,
    };
    use crate::provider_capabilities::ProviderEngine;
    use crate::transcription::error::{TranscriptionError, TranscriptionErrorCode};
    use crate::transcription::request::{
        CancellationToken, CleanupPolicy, EngineSelection, TimeoutPolicy,
    };
    use crate::transcription::{
        TranscriptionJob, TranscriptionResult, TranscriptionSource, TranscriptionTask,
    };
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    const CASES: [(bool, bool); 3] = [(true, false), (false, true), (false, false)];

    #[test]
    fn caller_owns_never_deletes() {
        for (success, retryable) in CASES {
            assert!(!should_delete_input(
                &CleanupPolicy::CallerOwns,
                success,
                retryable
            ));
        }
    }

    #[test]
    fn delete_after_attempt_always_deletes() {
        for (success, retryable) in CASES {
            assert!(should_delete_input(
                &CleanupPolicy::DeleteAfterAttempt,
                success,
                retryable
            ));
        }
    }

    #[test]
    fn preserve_on_retryable_keeps_only_retryable_failures() {
        // success -> delete
        assert!(should_delete_input(
            &CleanupPolicy::PreserveOnRetryableFailure,
            true,
            false
        ));
        // non-retryable failure -> delete
        assert!(should_delete_input(
            &CleanupPolicy::PreserveOnRetryableFailure,
            false,
            false
        ));
        // retryable failure -> keep for retry
        assert!(!should_delete_input(
            &CleanupPolicy::PreserveOnRetryableFailure,
            false,
            true
        ));
    }

    fn sample_job() -> TranscriptionJob {
        TranscriptionJob {
            source: TranscriptionSource::DesktopRecording,
            engine: "whisper".to_string(),
            model: "base".to_string(),
            spoken_language: None,
            task: TranscriptionTask::Transcribe,
        }
    }

    fn write_wav(path: &Path, sample_rate: u32, channels: u16, secs: f32) {
        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut writer = WavWriter::create(path, spec).unwrap();
        let frames = (secs * sample_rate as f32) as usize;
        for _ in 0..frames {
            for _ in 0..channels {
                writer.write_sample(0i16).unwrap();
            }
        }
        writer.finalize().unwrap();
    }

    #[tokio::test]
    async fn local_hard_timeout_returns_timeout_and_cancels_without_waiting_for_engine() {
        let cancellation = CancellationToken::new();
        let timed_out = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicBool::new(false));
        let started_in_engine = started.clone();

        let result = run_local_engine_with_hard_timeout(
            async move {
                started_in_engine.store(true, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(TranscriptionResult::new(&sample_job(), "late"))
            },
            Some(Duration::from_millis(5)),
            Duration::from_millis(5),
            cancellation.clone(),
            timed_out.clone(),
            TranscriptionSource::DesktopRecording,
        )
        .await;

        assert!(started.load(Ordering::SeqCst));
        assert!(cancellation.is_cancelled());
        assert!(timed_out.load(Ordering::SeqCst));
        assert_eq!(result.unwrap_err().code, TranscriptionErrorCode::Timeout);
    }

    #[tokio::test]
    async fn local_engine_success_within_grace_is_not_remapped_to_timeout() {
        // The cooperative watchdog has fired right around the decode's completion
        // — exactly the completion window the old in-helper Ok-path remap read
        // inside of. Simulate that by pre-setting the shared flag.
        let cancellation = CancellationToken::new();
        let timed_out = Arc::new(AtomicBool::new(true));

        // The decode completes successfully well within the hard deadline
        // (budget + grace), i.e. inside the grace window around the cooperative
        // deadline.
        let result = run_local_engine_with_hard_timeout(
            async { Ok(TranscriptionResult::new(&sample_job(), "decoded")) },
            Some(Duration::from_millis(50)),
            Duration::from_secs(2),
            cancellation.clone(),
            timed_out.clone(),
            TranscriptionSource::DesktopRecording,
        )
        .await;

        // A genuine successful decode must be returned as-is — the helper no
        // longer consults the watchdog flag for a completed result; the caller
        // remaps only after aborting the watchdog.
        let ok = result.expect("successful decode must not be remapped to Timeout");
        assert_eq!(ok.raw_text, "decoded");
        // The helper must not cancel on the Ok path — only its own hard-timeout
        // branch cancels.
        assert!(
            !cancellation.is_cancelled(),
            "helper must not cancel on a completed result"
        );
    }

    #[test]
    fn remap_timed_out_relabels_cancelled_as_timeout_only_when_fired() {
        let err = || {
            Err::<TranscriptionResult, _>(TranscriptionError::new(
                TranscriptionErrorCode::Cancelled,
                TranscriptionSource::DesktopRecording,
                "Transcription cancelled",
            ))
        };

        // Watchdog fired: a cancellation is really a timeout.
        let remapped = remap_timed_out(err(), true, TranscriptionSource::DesktopRecording);
        assert_eq!(remapped.unwrap_err().code, TranscriptionErrorCode::Timeout);

        // No watchdog: a real user cancellation stays cancelled.
        let kept = remap_timed_out(err(), false, TranscriptionSource::DesktopRecording);
        assert_eq!(kept.unwrap_err().code, TranscriptionErrorCode::Cancelled);
    }

    #[test]
    fn remap_timed_out_maps_every_result_to_timeout_when_fired() {
        let cases = [
            Ok(TranscriptionResult::new(&sample_job(), "hi")),
            Err(TranscriptionError::new(
                TranscriptionErrorCode::Cancelled,
                TranscriptionSource::DesktopRecording,
                "Transcription cancelled",
            )),
            Err(TranscriptionError::new(
                TranscriptionErrorCode::EngineFailed,
                TranscriptionSource::DesktopRecording,
                "boom",
            )),
        ];

        for result in cases {
            let remapped = remap_timed_out(result, true, TranscriptionSource::DesktopRecording);
            assert_eq!(remapped.unwrap_err().code, TranscriptionErrorCode::Timeout);
        }

        let kept_ok = remap_timed_out(
            Ok(TranscriptionResult::new(&sample_job(), "hi")),
            false,
            TranscriptionSource::DesktopRecording,
        )
        .unwrap();
        assert_eq!(kept_ok.raw_text, "hi");

        for code in [
            TranscriptionErrorCode::Cancelled,
            TranscriptionErrorCode::EngineFailed,
        ] {
            let kept = remap_timed_out(
                Err(TranscriptionError::new(
                    code,
                    TranscriptionSource::DesktopRecording,
                    "original",
                )),
                false,
                TranscriptionSource::DesktopRecording,
            );
            assert_eq!(kept.unwrap_err().code, code);
        }
    }

    #[test]
    fn explicit_remote_selection_returns_clear_executor_error() {
        let error = deferred_executor_route_error(
            &EngineSelection::Explicit {
                engine: ProviderEngine::Remote,
                model: "remote_peer".to_string(),
            },
            TranscriptionSource::DesktopRecording,
        )
        .expect("remote selection should be rejected before executor resolution");

        assert_eq!(error.code, TranscriptionErrorCode::Internal);
        assert_eq!(error.source, TranscriptionSource::DesktopRecording);
        assert!(error
            .user_message
            .contains("outside the transcription executor"));
        assert!(error
            .user_message
            .contains("desktop remote transcription path"));
    }

    #[test]
    fn is_normalized_wav_accepts_only_16k_mono_s16() {
        let conforming = NamedTempFile::new().unwrap();
        write_wav(conforming.path(), 16_000, 1, 0.1);
        assert!(is_normalized_wav(conforming.path()));

        let wrong_rate = NamedTempFile::new().unwrap();
        write_wav(wrong_rate.path(), 44_100, 1, 0.1);
        assert!(!is_normalized_wav(wrong_rate.path()));

        let stereo = NamedTempFile::new().unwrap();
        write_wav(stereo.path(), 16_000, 2, 0.1);
        assert!(!is_normalized_wav(stereo.path()));

        assert!(!is_normalized_wav(Path::new("/nonexistent/file.wav")));
    }

    #[test]
    fn wav_duration_ms_reads_clip_length() {
        let tmp = NamedTempFile::new().unwrap();
        write_wav(tmp.path(), 16_000, 1, 1.0);
        let ms = wav_duration_ms(tmp.path()).expect("duration");
        assert!((990..=1010).contains(&ms), "expected ~1000ms, got {ms}");
        assert!(wav_duration_ms(Path::new("/nonexistent/file.wav")).is_none());
    }

    #[test]
    fn watchdog_budget_for_honors_policy() {
        let tmp = NamedTempFile::new().unwrap();
        write_wav(tmp.path(), 16_000, 1, 1.0);

        assert!(watchdog_budget_for(tmp.path(), &TimeoutPolicy::None).is_none());
        assert_eq!(
            watchdog_budget_for(
                tmp.path(),
                &TimeoutPolicy::Explicit(Duration::from_secs(42))
            ),
            Some(Duration::from_secs(42))
        );
        let budget = watchdog_budget_for(tmp.path(), &TimeoutPolicy::Interactive).unwrap();
        assert!(budget >= Duration::from_secs(180));
    }

    #[test]
    fn effective_parakeet_audio_duration_ms_fallback_cases() {
        // Write a 1-second 16 kHz mono WAV.
        let tmp = NamedTempFile::new().unwrap();
        write_wav(tmp.path(), 16_000, 1, 1.0);

        // Sidecar reports a positive value — trust it.
        assert_eq!(
            effective_parakeet_audio_duration_ms(Some(5.0), tmp.path()),
            Some(5000)
        );

        // Sidecar reports zero — fall back to WAV header (~1000 ms).
        let from_zero = effective_parakeet_audio_duration_ms(Some(0.0), tmp.path())
            .expect("should fall back to WAV header");
        assert!(
            (990..=1010).contains(&from_zero),
            "expected ~1000 ms from WAV header, got {from_zero}"
        );

        // Sidecar absent — fall back to WAV header (~1000 ms).
        let from_none = effective_parakeet_audio_duration_ms(None, tmp.path())
            .expect("should fall back to WAV header");
        assert!(
            (990..=1010).contains(&from_none),
            "expected ~1000 ms from WAV header, got {from_none}"
        );

        // Sidecar absent + nonexistent path — returns None, no panic.
        assert_eq!(
            effective_parakeet_audio_duration_ms(None, Path::new("/nonexistent/file.wav")),
            None
        );
    }
    #[test]
    fn cloud_translate_to_english_rejected_for_unsupported_engines() {
        use crate::cloud_stt::CloudProvider;

        // No curated cloud provider supports translation; each must reject a
        // translate-to-English request instead of returning source-language
        // text that TranscriptionResult::new would mislabel as English.
        for provider in CloudProvider::ALL {
            let err = ensure_cloud_task_supported(*provider, true, TranscriptionSource::AudioFile)
                .expect_err("translate-to-English must be rejected on cloud");
            assert_eq!(err.code, TranscriptionErrorCode::EngineUnavailable);
            assert!(
                !err.retryable,
                "an unsupported task must not be flagged retryable"
            );
            assert!(
                err.user_message.contains("translate to English"),
                "message should name the unsupported task: {}",
                err.user_message
            );
            assert!(
                err.user_message.contains(provider.display_name()),
                "message should name the provider: {}",
                err.user_message
            );
        }

        // A plain transcribe task is always accepted on every cloud provider.
        for provider in CloudProvider::ALL {
            ensure_cloud_task_supported(*provider, false, TranscriptionSource::AudioFile)
                .expect("transcribe task must be accepted on cloud");
        }
    }
}
