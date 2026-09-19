use std::path::Path;

tokio::task_local! {
    /// Backend selected for this transcription attempt, recorded before initialization.
    /// A failed initialization retains its attempted backend; unrelated tasks cannot
    /// overwrite it. Reads outside the recording's scope return `None`.
    pub(crate) static ATTEMPT_BACKEND: std::cell::Cell<Option<&'static str>>;
}

pub(crate) fn record_attempt_backend(backend: &'static str) {
    let _ = ATTEMPT_BACKEND.try_with(|slot| slot.set(Some(backend)));
}

pub(crate) fn attempt_backend() -> Option<&'static str> {
    ATTEMPT_BACKEND.try_with(|slot| slot.get()).ok().flatten()
}

use std::time::Instant;
use whisper_rs::{
    convert_integer_to_float_audio, convert_stereo_to_mono_audio, FullParams, SamplingStrategy,
    WhisperContext, WhisperContextParameters,
};

use crate::utils::logger::*;
#[cfg(debug_assertions)]
use crate::utils::system_monitor;

pub struct Transcriber {
    context: WhisperContext,
    cpu_profile: bool,
    /// Acceleration this loaded instance serves ("cpu" | "metal"); the
    /// attempt records it when the instance is used (plan 060.1).
    backend: &'static str,
}
#[derive(Debug, Clone, PartialEq)]
pub struct WhisperTranscriptionOutput {
    pub raw_text: String,
    pub transcript_language: Option<String>,
    pub segments: Vec<crate::transcription::TranscriptionSegment>,
    pub audio_duration_ms: u64,
    pub processing_duration_ms: u64,
}

impl Transcriber {
    /// Acceleration label this loaded instance serves (plan 060.1).
    pub(crate) fn backend(&self) -> &'static str {
        self.backend
    }

    pub fn new(model_path: &Path) -> Result<Self, String> {
        let init_start = Instant::now();
        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| format!("Model path contains invalid UTF-8: {:?}", model_path))?;

        log_start("TRANSCRIBER_INIT");
        log_with_context(
            log::Level::Debug,
            "Initializing transcriber",
            &[
                ("model_path", model_path_str),
                ("platform", std::env::consts::OS),
            ],
        );

        // Log model file info
        if let Ok(metadata) = std::fs::metadata(model_path) {
            let size_mb = metadata.len() / 1024 / 1024;
            log_file_operation(
                "MODEL_LOAD",
                model_path_str,
                true,
                Some(metadata.len()),
                None,
            );
            log::info!("🤖 Model file size: {}MB", size_mb);
        }

        // Configure GPU usage based on platform and features
        let mut ctx_params = WhisperContextParameters::default();

        // macOS: Try Metal first, fallback to CPU if it fails
        // Note: Metal GPU acceleration only works on Apple Silicon (aarch64), not Intel Macs
        #[cfg(target_os = "macos")]
        {
            let is_apple_silicon = std::env::consts::ARCH == "aarch64";

            if !is_apple_silicon {
                log_with_context(
                    log::Level::Info,
                    "🎮 METAL_SKIP",
                    &[
                        ("reason", "Intel Mac detected"),
                        ("arch", std::env::consts::ARCH),
                        ("fallback_to", "CPU"),
                    ],
                );
                log::info!("⚠️ Intel Mac detected - using CPU-only mode (Metal GPU not supported)");
                ctx_params.use_gpu(false);
            } else {
                ctx_params.use_gpu(true);
            }

            let metal_start = Instant::now();

            log_with_context(
                log::Level::Info,
                "🎮 METAL_INIT",
                &[
                    ("backend", if is_apple_silicon { "Metal" } else { "CPU" }),
                    ("platform", "macOS"),
                    ("arch", std::env::consts::ARCH),
                    (
                        "attempt",
                        if is_apple_silicon {
                            "gpu_first"
                        } else {
                            "cpu_only"
                        },
                    ),
                ],
            );

            // Retain the selected attempt even when context initialization fails.
            record_attempt_backend(if is_apple_silicon { "metal" } else { "cpu" });
            match WhisperContext::new_with_params(model_path_str, ctx_params) {
                Ok(ctx) => {
                    let cpu_profile = !is_apple_silicon;
                    let init_time = metal_start.elapsed().as_millis();
                    let backend_type = if cpu_profile { "CPU" } else { "Metal GPU" };
                    let acceleration = if cpu_profile { "disabled" } else { "enabled" };

                    log_performance(
                        "METAL_INIT",
                        init_time as u64,
                        Some(if cpu_profile {
                            "cpu_only"
                        } else {
                            "gpu_acceleration_enabled"
                        }),
                    );
                    log_with_context(
                        log::Level::Info,
                        if cpu_profile {
                            "🎮 CPU_INIT"
                        } else {
                            "🎮 METAL_SUCCESS"
                        },
                        &[
                            ("init_time_ms", init_time.to_string().as_str()),
                            ("acceleration", acceleration),
                        ],
                    );

                    log_complete("TRANSCRIBER_INIT", init_start.elapsed().as_millis() as u64);
                    log_with_context(
                        log::Level::Debug,
                        "Transcriber initialized",
                        &[("backend", backend_type), ("model_path", model_path_str)],
                    );

                    let backend = if cpu_profile { "cpu" } else { "metal" };
                    return Ok(Self {
                        context: ctx,
                        cpu_profile,
                        backend,
                    });
                }
                Err(gpu_err) => {
                    log_with_context(
                        log::Level::Info,
                        "🎮 METAL_FALLBACK",
                        &[
                            ("error", gpu_err.to_string().as_str()),
                            ("fallback_to", "CPU"),
                            (
                                "attempt_time_ms",
                                metal_start.elapsed().as_millis().to_string().as_str(),
                            ),
                        ],
                    );

                    ctx_params = WhisperContextParameters::default();
                    ctx_params.use_gpu(false);
                    log::info!("🔄 Attempting CPU-only initialization...");
                }
            }
        }

        // Windows: CPU-only in main binary. GPU runs in whisper-vulkan-sidecar process.
        #[cfg(target_os = "windows")]
        {
            log::info!("Windows: Using CPU-only Whisper path (GPU via sidecar)");
            ctx_params.use_gpu(false);
        }

        // Create context (for Windows CPU fallback or other platforms)
        let cpu_start = Instant::now();
        record_attempt_backend("cpu");
        let ctx = WhisperContext::new_with_params(model_path_str, ctx_params).map_err(|e| {
            log_failed("TRANSCRIBER_INIT", &e.to_string());
            log_with_context(
                log::Level::Debug,
                "CPU fallback failed",
                &[("model_path", model_path_str), ("backend", "CPU_FALLBACK")],
            );
            format!("Failed to load model: {}", e)
        })?;

        let backend_type = "CPU";

        let cpu_time = cpu_start.elapsed().as_millis();

        log_with_context(
            log::Level::Info,
            "🎮 WHISPER_BACKEND",
            &[
                ("backend", backend_type),
                ("gpu_used", "false"),
                ("init_time_ms", cpu_time.to_string().as_str()),
            ],
        );

        log_complete("TRANSCRIBER_INIT", init_start.elapsed().as_millis() as u64);
        log_with_context(
            log::Level::Debug,
            "Transcriber initialization complete",
            &[
                ("backend", backend_type),
                ("model_path", model_path_str),
                ("gpu_acceleration", "false"),
            ],
        );

        log::info!(
            "✅ Whisper initialized successfully using {} backend",
            backend_type
        );

        // Log model capabilities and validation
        log_with_context(
            log::Level::Debug,
            "Model initialization validation",
            &[
                ("model_loaded", "true"),
                ("backend", backend_type),
                ("supports_multilingual", "true"), // Whisper models are multilingual
                (
                    "model_size_mb",
                    (std::fs::metadata(model_path)
                        .map(|m| m.len() / 1024 / 1024)
                        .unwrap_or(0)
                        .to_string())
                    .as_str(),
                ),
            ],
        );

        Ok(Self {
            context: ctx,
            cpu_profile: true,
            backend: "cpu",
        })
    }

    #[allow(dead_code)]
    pub fn transcribe_with_translation(
        &self,
        audio_path: &Path,
        language: Option<&str>,
        translate: bool,
    ) -> Result<String, String> {
        self.transcribe_with_metadata(audio_path, language, translate, || false)
            .map(|result| result.raw_text)
    }

    #[allow(dead_code)]
    pub fn transcribe_with_cancellation<F>(
        &self,
        audio_path: &Path,
        language: Option<&str>,
        translate: bool,
        should_cancel: F,
    ) -> Result<String, String>
    where
        F: Fn() -> bool + Clone + 'static,
    {
        self.transcribe_with_metadata(audio_path, language, translate, should_cancel)
            .map(|result| result.raw_text)
    }

    pub fn transcribe_with_metadata<F>(
        &self,
        audio_path: &Path,
        language: Option<&str>,
        translate: bool,
        should_cancel: F,
    ) -> Result<WhisperTranscriptionOutput, String>
    where
        F: Fn() -> bool + Clone + 'static,
    {
        self.transcribe_with_metadata_with_prompt(
            audio_path,
            language,
            translate,
            None,
            should_cancel,
        )
    }

    pub fn transcribe_with_metadata_with_prompt<F>(
        &self,
        audio_path: &Path,
        language: Option<&str>,
        translate: bool,
        initial_prompt: Option<&str>,
        should_cancel: F,
    ) -> Result<WhisperTranscriptionOutput, String>
    where
        F: Fn() -> bool + Clone + 'static,
    {
        let transcription_start = Instant::now();
        let audio_path_str = format!("{:?}", audio_path);

        // Monitor system resources before transcription (only in debug builds)
        #[cfg(debug_assertions)]
        system_monitor::log_resources_before_operation("TRANSCRIPTION");

        log_start("TRANSCRIPTION");
        log_with_context(
            log::Level::Debug,
            "Starting transcription",
            &[
                ("audio_path", &audio_path_str),
                ("language", language.unwrap_or("auto")),
                ("translate", translate.to_string().as_str()),
                ("timestamp", chrono::Utc::now().to_rfc3339().as_str()),
            ],
        );

        // Check if file exists and is readable
        if !audio_path.exists() {
            let error = format!("Audio file does not exist: {:?}", audio_path);
            log_failed("TRANSCRIPTION", &error);
            log_with_context(
                log::Level::Debug,
                "File validation failed",
                &[
                    ("stage", "file_validation"),
                    ("audio_path", &audio_path_str),
                ],
            );
            return Err(error);
        }

        // Early cancellation check
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled before starting");
            return Err("Transcription cancelled".to_string());
        }

        let file_size = std::fs::metadata(audio_path)
            .map_err(|e| {
                let error = format!("Cannot read file metadata: {}", e);
                log_failed("TRANSCRIPTION", &error);
                log_with_context(
                    log::Level::Debug,
                    "Metadata read failed",
                    &[("stage", "metadata_read"), ("audio_path", &audio_path_str)],
                );
                error
            })?
            .len();

        log_file_operation(
            "TRANSCRIPTION_INPUT",
            &audio_path_str,
            true,
            Some(file_size),
            None,
        );

        if file_size == 0 {
            let error = "Audio file is empty (0 bytes)";
            log_failed("TRANSCRIPTION", error);
            log_with_context(
                log::Level::Debug,
                "File size check failed",
                &[("stage", "file_size_check"), ("file_size", "0")],
            );
            return Err(error.to_string());
        }

        // Read WAV file
        let audio_read_start = Instant::now();
        let mut reader = hound::WavReader::open(audio_path).map_err(|e| {
            let error = format!("Failed to open WAV file: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);

            error
        })?;

        let spec = reader.spec();
        log::info!(
            "[TRANSCRIPTION_DEBUG] WAV spec: channels={}, sample_rate={}, bits={}",
            spec.channels,
            spec.sample_rate,
            spec.bits_per_sample
        );

        /* ----------------------------------------------
        1) read raw i16 pcm
        ---------------------------------------------- */
        let samples_i16: Vec<i16> = reader
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read audio samples: {}", e))?;

        // Check cancellation after reading samples
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after reading samples");
            return Err("Transcription cancelled".to_string());
        }

        /* ----------------------------------------------
        2) i16 → f32  (range -1.0 … 1.0)
        ---------------------------------------------- */
        let mut audio: Vec<f32> = vec![0.0; samples_i16.len()];
        convert_integer_to_float_audio(&samples_i16, &mut audio).map_err(|e| e.to_string())?;

        // Check cancellation after conversion
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after audio conversion");
            return Err("Transcription cancelled".to_string());
        }

        /* ----------------------------------------------
        3) multi-channel → mono  (Whisper needs mono)
        ---------------------------------------------- */
        if spec.channels == 2 {
            let mut mono_audio = vec![0.0; audio.len() / 2];
            convert_stereo_to_mono_audio(&audio, &mut mono_audio).map_err(|e| e.to_string())?;
            audio = mono_audio;
        } else if spec.channels > 2 {
            // Handle multi-channel audio (3, 4, 5.1, 7.1, etc.)
            log::info!(
                "[TRANSCRIPTION_DEBUG] Converting {}-channel audio to mono",
                spec.channels
            );
            audio = convert_multichannel_to_mono(&audio, spec.channels as usize)?;
        } else if spec.channels != 1 {
            return Err(format!("Invalid channel count: {}", spec.channels));
        }

        // Store original audio length before the move
        let _original_audio_length = audio.len();

        /* ----------------------------------------------
        4) Resample to 16kHz using high-quality resampler
        ---------------------------------------------- */
        // Use rubato for high-quality resampling to 16kHz
        let resampled_audio = if spec.sample_rate != 16_000 {
            use crate::audio::resampler::resample_to_16khz;

            log::info!(
                "[TRANSCRIPTION_DEBUG] Resampling audio from {} Hz to 16000 Hz",
                spec.sample_rate
            );

            resample_to_16khz(&audio, spec.sample_rate)?
        } else {
            log::info!("[TRANSCRIPTION_DEBUG] Audio already at 16kHz, no resampling needed");
            audio
        };

        // Log audio preprocessing performance
        let preprocessing_time = audio_read_start.elapsed().as_millis() as u64;
        log_performance(
            "AUDIO_PREPROCESSING",
            preprocessing_time,
            Some(&format!("samples={}", resampled_audio.len())),
        );
        log_with_context(
            log::Level::Debug,
            "Audio preprocessing complete",
            &[
                (
                    "preprocessing_time_ms",
                    preprocessing_time.to_string().as_str(),
                ),
                ("sample_rate", "16000"),
                ("channels", "1"),
                ("samples", resampled_audio.len().to_string().as_str()),
            ],
        );

        // Check cancellation after resampling
        if should_cancel() {
            log::info!("[TRANSCRIPTION_DEBUG] Transcription cancelled after resampling");
            return Err("Transcription cancelled".to_string());
        }

        log::debug!(
            "Audio ready for Whisper: {} samples at 16kHz ({:.2}s)",
            resampled_audio.len(),
            resampled_audio.len() as f32 / 16_000_f32
        );

        let mut params = if self.cpu_profile {
            log::info!("[PERFORMANCE] Using CPU fast transcription profile");
            FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
        } else {
            FullParams::new(SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: -1.0,
            })
        };

        // Set language - use centralized validation
        log::info!("[LANGUAGE] Received language: {:?}", language);

        let final_lang = if let Some(lang) = language {
            if lang == "auto" {
                // Auto-detect removed due to 30-second requirement, default to English
                log::info!("[LANGUAGE] Auto-detection no longer supported, defaulting to English");
                Some("en")
            } else {
                let validated = super::languages::validate_language(Some(lang));
                log::info!("[LANGUAGE] Using language: {}", validated);
                Some(validated)
            }
        } else {
            log::info!("[LANGUAGE] No language specified, using English");
            Some("en")
        };

        if let Some(lang) = final_lang {
            log::info!("[LANGUAGE] Final language set to: {}", lang);
            params.set_language(Some(lang));
        }

        // Set translate mode
        if translate {
            log::info!("[LANGUAGE] Translation mode enabled - will translate to English");
            params.set_translate(true);
        } else {
            log::info!("[LANGUAGE] Transcription mode - will transcribe in original language");
            params.set_translate(false);
        }

        // Use most cores but leave one free to keep UI responsive
        let hw = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        // ARM big.LITTLE optimization: On ARM64 Windows (Qualcomm Snapdragon),
        // using all cores is slower because efficiency cores drag down performance.
        // Limit to ~4 threads (performance cores only) for better speed.
        #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
        let threads = {
            let t = std::cmp::min(4, std::cmp::max(1, hw.saturating_sub(1))) as i32;
            log::info!(
                "[PERFORMANCE] ARM64: Using {} threads (limiting to perf cores, {} total available)",
                t,
                hw
            );
            t
        };

        #[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
        let threads = {
            let t = std::cmp::max(1, hw.saturating_sub(1)) as i32;
            log::info!("[PERFORMANCE] Using {} threads for transcription", t);
            t
        };

        params.set_n_threads(threads);

        params.set_no_context(self.cpu_profile);
        params.set_no_timestamps(self.cpu_profile);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Suppress blank outputs to avoid empty transcriptions
        params.set_suppress_blank(true);

        // Suppress annotation-like tokens while keeping commas and sentence-ending punctuation.
        params.set_suppress_nst(true);

        // Adjust speech detection threshold
        params.set_no_speech_thold(0.6); // Default value, as higher values can cause issues

        // Quality thresholds - use more lenient values to avoid rejecting valid speech
        // Default entropy threshold is 2.4, we'll keep it default to avoid over-filtering
        params.set_entropy_thold(2.4); // Default value - prevents filtering out valid but uncertain speech

        // Use default log probability threshold to avoid being too strict
        params.set_logprob_thold(-1.0); // Default value - balanced probability requirements

        params.set_initial_prompt(initial_prompt.unwrap_or(""));

        params.set_temperature(if self.cpu_profile { 0.0 } else { 0.2 });
        params.set_temperature_inc(0.2); // Increase by 0.2 on fallback (default)
        params.set_max_initial_ts(1.0); // Limit initial timestamp search

        // Limit segment length to prevent runaway hallucinations
        params.set_max_len(0); // 0 means no limit
        params.set_length_penalty(-1.0); // Default penalty

        // Run transcription
        log::info!("[TRANSCRIPTION_DEBUG] Creating Whisper state...");
        let mut state = self.context.create_state().map_err(|e| {
            let error = format!("Failed to create Whisper state: {}", e);
            log::error!("[TRANSCRIPTION_DEBUG] {}", error);

            error
        })?;

        let samples_count = resampled_audio.len();
        let duration_seconds = samples_count as f32 / 16_000_f32;

        // Check minimum duration (0.5 seconds)
        if duration_seconds < 0.5 {
            let error = "Recording too short".to_string();
            log::warn!("[TRANSCRIPTION_DEBUG] {}", error);
            return Err(error);
        }

        let inference_start = Instant::now();
        log_start("WHISPER_INFERENCE");
        log_with_context(
            log::Level::Debug,
            "Starting Whisper inference",
            &[
                ("samples", samples_count.to_string().as_str()),
                (
                    "duration_seconds",
                    format!("{:.2}", duration_seconds).as_str(),
                ),
                ("language", language.unwrap_or("auto")),
                ("translate", translate.to_string().as_str()),
            ],
        );

        match full_with_cancel(&mut state, params, &resampled_audio, should_cancel.clone()) {
            Ok(_) => {
                let inference_time = inference_start.elapsed();
                let inference_ms = inference_time.as_millis();

                log_performance(
                    "WHISPER_INFERENCE",
                    inference_ms as u64,
                    Some(&format!(
                        "audio_duration={:.2}s, samples={}",
                        duration_seconds, samples_count
                    )),
                );

                log::info!(
                    "✅ Whisper inference completed in {:.2}s",
                    inference_time.as_secs_f32()
                );
            }
            Err(e) => {
                if should_cancel() {
                    log::info!("[TRANSCRIPTION_DEBUG] Whisper inference was cancelled");
                    return Err("Transcription cancelled".to_string());
                }

                let error = format!("Whisper inference failed: {}", e);
                log_failed("WHISPER_INFERENCE", &error);
                log_with_context(
                    log::Level::Debug,
                    "Inference failed",
                    &[
                        ("samples", samples_count.to_string().as_str()),
                        (
                            "duration_seconds",
                            format!("{:.2}", duration_seconds).as_str(),
                        ),
                        (
                            "inference_time_ms",
                            inference_start.elapsed().as_millis().to_string().as_str(),
                        ),
                    ],
                );
                return Err(error);
            }
        }

        // Get text
        let text_extraction_start = Instant::now();
        log::info!("[TRANSCRIPTION_DEBUG] Getting segments from Whisper output...");
        let num_segments = state.full_n_segments();

        log::info!(
            "[TRANSCRIPTION_DEBUG] Transcription complete: {} segments",
            num_segments
        );

        let mut text = String::new();
        let mut segments = Vec::with_capacity(num_segments as usize);
        for (i, segment) in state.as_iter().enumerate() {
            let segment_text = segment.to_string();
            log::debug!(
                "[TRANSCRIPTION_DEBUG] Segment {}: {} chars",
                i,
                segment_text.chars().count()
            );
            text.push_str(&segment_text);
            text.push(' ');
            segments.push(crate::transcription::TranscriptionSegment {
                text: segment_text,
                start_ms: None,
                end_ms: None,
                speaker_id: None,
            });
        }

        let trimmed = text.trim();
        let result = transcript_text::normalize_transcript_spacing(trimmed).into_owned();

        // Log text extraction performance
        let extraction_time = text_extraction_start.elapsed().as_millis() as u64;
        log_performance(
            "TEXT_EXTRACTION",
            extraction_time,
            Some(&format!(
                "segments={}, chars={}",
                num_segments,
                result.len()
            )),
        );

        let total_time = transcription_start.elapsed();

        // Log system resources after transcription (only in debug builds)
        #[cfg(debug_assertions)]
        system_monitor::log_resources_after_operation(
            "TRANSCRIPTION",
            total_time.as_millis() as u64,
        );

        if result.is_empty() {
            log_failed("TRANSCRIPTION", "Empty transcription result");
            log_with_context(
                log::Level::Debug,
                "Empty result",
                &[
                    ("segments", num_segments.to_string().as_str()),
                    ("total_time_ms", total_time.as_millis().to_string().as_str()),
                ],
            );
        } else if result == "[SOUND]" {
            log_failed("TRANSCRIPTION", "No speech detected");
            log_with_context(
                log::Level::Debug,
                "No speech",
                &[
                    ("result", "[SOUND]"),
                    ("segments", num_segments.to_string().as_str()),
                    ("total_time_ms", total_time.as_millis().to_string().as_str()),
                ],
            );
        } else {
            log_complete("TRANSCRIPTION", total_time.as_millis() as u64);
            log_with_context(
                log::Level::Debug,
                "Transcription complete",
                &[
                    ("result_length", result.len().to_string().as_str()),
                    ("segments", num_segments.to_string().as_str()),
                    (
                        "audio_duration_seconds",
                        format!("{:.2}", duration_seconds).as_str(),
                    ),
                ],
            );

            log::info!(
                "📝 Transcription success: {} chars in {:.2}s",
                result.len(),
                total_time.as_secs_f32()
            );
        }

        let transcript_language = if translate {
            Some("en".to_string())
        } else {
            final_lang.map(str::to_string)
        };

        Ok(WhisperTranscriptionOutput {
            raw_text: result,
            transcript_language,
            segments,
            audio_duration_ms: (duration_seconds * 1000.0) as u64,
            processing_duration_ms: total_time.as_millis() as u64,
        })
    }
}

unsafe extern "C" fn whisper_abort_callback<F: Fn() -> bool>(
    user_data: *mut std::ffi::c_void,
) -> bool {
    // SAFETY: full_with_cancel passes a live F, not a boxed trait object's address.
    let should_cancel = unsafe { &*user_data.cast::<F>() };
    should_cancel()
}

fn full_with_cancel<F: Fn() -> bool + 'static>(
    state: &mut whisper_rs::WhisperState,
    mut params: FullParams<'_, '_>,
    audio: &[f32],
    should_cancel: F,
) -> Result<(), whisper_rs::WhisperError> {
    // whisper-rs 0.16.0's safe helper casts Box<dyn FnMut()> storage to F,
    // causing undefined behavior (including spurious encoder -6) and leaking it.
    // SAFETY: this concrete F stays alive and unmoved until synchronous full()
    // returns. whisper.cpp polls on the calling thread and does not retain the
    // callback beyond full(). Neither the callback nor its data escapes here.
    unsafe {
        params.set_abort_callback(Some(whisper_abort_callback::<F>));
        params.set_abort_callback_user_data(std::ptr::from_ref(&should_cancel).cast_mut().cast());
    }
    state.full(params, audio)
}

/// Convert multi-channel audio to mono by averaging all channels
///
/// # Arguments
/// * `audio` - Interleaved audio samples (ch1, ch2, ch3, ch4, ch1, ch2, ...)
/// * `channels` - Number of channels in the audio
///
/// # Returns
/// Mono audio with averaged samples from all channels
fn convert_multichannel_to_mono(audio: &[f32], channels: usize) -> Result<Vec<f32>, String> {
    if channels == 0 {
        return Err("Channel count cannot be zero".to_string());
    }

    if channels == 1 {
        // Already mono, just return a copy
        return Ok(audio.to_vec());
    }

    let samples_per_channel = audio.len() / channels;
    let mut mono_audio = Vec::with_capacity(samples_per_channel);

    // Process each frame (set of samples across all channels)
    for i in 0..samples_per_channel {
        let mut sum = 0.0f32;

        // Sum all channels for this sample position
        for ch in 0..channels {
            let idx = i * channels + ch;
            if idx < audio.len() {
                sum += audio[idx];
            }
        }

        // Average the channels
        mono_audio.push(sum / channels as f32);
    }

    log::info!(
        "[AUDIO] Downmixed {}-channel audio to mono: {} samples -> {} samples",
        channels,
        audio.len(),
        mono_audio.len()
    );

    Ok(mono_audio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_abort_callback_reads_live_captured_token() {
        use crate::transcription::request::CancellationToken;
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        fn invoke<F: Fn() -> bool>(callback: &F) -> bool {
            // SAFETY: the typed callback remains borrowed for this entire call.
            unsafe { whisper_abort_callback::<F>(std::ptr::from_ref(callback).cast_mut().cast()) }
        }

        let flag = Arc::new(AtomicBool::new(false));
        let token = CancellationToken::from_arc(flag.clone());
        let callback = move || token.is_cancelled();
        assert!(!invoke(&callback));
        flag.store(true, Ordering::SeqCst);
        assert!(invoke(&callback));
        flag.store(false, Ordering::SeqCst);
        assert!(!invoke(&callback));
        drop(callback);
        assert_eq!(Arc::strong_count(&flag), 1);
    }

    #[test]
    #[ignore = "requires VOICETYPR_TEST_WHISPER_MODEL pointing to Base English weights"]
    fn real_engine_captured_cancellation_and_context_reuse() {
        use crate::transcription::request::CancellationToken;
        use std::sync::{atomic::AtomicBool, Arc};

        let model = std::env::var("VOICETYPR_TEST_WHISPER_MODEL").expect("set model path");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/audio-files/test-audio.wav");
        let mut reader = hound::WavReader::open(fixture).unwrap();
        assert_eq!(reader.spec().sample_rate, 16000);
        assert_eq!(reader.spec().channels, 1);
        let audio: Vec<f32> = reader
            .samples::<i16>()
            .take(53931)
            .map(|sample| sample.unwrap() as f32 / 32768.0)
            .collect();
        assert_eq!(audio.len(), 53931);
        for use_gpu in [true, false] {
            let mut context_params = WhisperContextParameters::default();
            context_params.use_gpu(use_gpu);
            let context = WhisperContext::new_with_params(&model, context_params).unwrap();
            for cancelled in [false, true, false] {
                let flag = Arc::new(AtomicBool::new(cancelled));
                let token = CancellationToken::from_arc(flag.clone());
                let mut params = FullParams::new(SamplingStrategy::BeamSearch {
                    beam_size: 5,
                    patience: -1.0,
                });
                params.set_language(Some("en"));
                params.set_n_threads(11);
                params.set_print_progress(false);
                params.set_print_realtime(false);
                let mut state = context.create_state().unwrap();
                let result =
                    full_with_cancel(&mut state, params, &audio, move || token.is_cancelled());
                if cancelled {
                    assert!(
                        matches!(result, Err(whisper_rs::WhisperError::GenericError(-6))),
                        "{result:?}"
                    );
                } else {
                    result.unwrap();
                    let text: String = state
                        .as_iter()
                        .map(|segment| segment.to_str_lossy().unwrap().into_owned())
                        .collect();
                    assert!(text.to_lowercase().contains("testing"), "{text}");
                }
                assert_eq!(Arc::strong_count(&flag), 1, "callback capture leaked");
            }
        }
    }

    #[test]
    fn test_convert_multichannel_to_mono() {
        // Test 4-channel audio downmixing
        // Simulating interleaved 4-channel audio: [ch1, ch2, ch3, ch4, ch1, ch2, ...]
        let four_channel_audio = vec![
            1.0, 2.0, 3.0, 4.0, // Frame 1: channels 1-4
            5.0, 6.0, 7.0, 8.0, // Frame 2: channels 1-4
            -1.0, -2.0, -3.0, -4.0, // Frame 3: channels 1-4
        ];

        let result = convert_multichannel_to_mono(&four_channel_audio, 4).unwrap();

        // Expected: average of each frame's channels
        // Frame 1: (1+2+3+4)/4 = 2.5
        // Frame 2: (5+6+7+8)/4 = 6.5
        // Frame 3: (-1-2-3-4)/4 = -2.5
        assert_eq!(result.len(), 3);
        assert!((result[0] - 2.5).abs() < 0.001);
        assert!((result[1] - 6.5).abs() < 0.001);
        assert!((result[2] - (-2.5)).abs() < 0.001);
    }

    #[test]
    fn test_convert_stereo_passthrough() {
        // Test that mono audio passes through unchanged
        let mono_audio = vec![1.0, 2.0, 3.0, 4.0];
        let result = convert_multichannel_to_mono(&mono_audio, 1).unwrap();
        assert_eq!(result, mono_audio);
    }

    #[test]
    fn test_convert_invalid_channels() {
        // Test that zero channels returns an error
        let audio = vec![1.0, 2.0];
        let result = convert_multichannel_to_mono(&audio, 0);
        assert!(result.is_err());
    }
    #[test]
    fn attempt_backend_is_none_outside_any_scope() {
        // A failure event emitted without a scoped attempt (the scope wraps
        // the spawned decode future) must observe NO backend — never a
        // previous task's — so the report omits the tag instead of guessing.
        assert_eq!(attempt_backend(), None);
        record_attempt_backend("sidecar"); // no-op: no scope on this task
        assert_eq!(attempt_backend(), None);
    }

    #[tokio::test]
    async fn failed_cold_initialization_records_cpu_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let missing_model = directory.path().join("missing-model.bin");
        // Whisper opens the model before initializing any device. A missing
        // file exercises real initialization failure without GPU/model assets.
        ATTEMPT_BACKEND
            .scope(std::cell::Cell::new(None), async {
                assert!(Transcriber::new(&missing_model).is_err());
                // On Apple Silicon the Metal attempt fails first, then CPU;
                // other platforms attempt CPU directly.
                assert_eq!(attempt_backend(), Some("cpu"));
            })
            .await;
        assert_eq!(attempt_backend(), None);
    }

    #[tokio::test]
    async fn failed_unscoped_preload_does_not_change_recording_backend() {
        let directory = tempfile::tempdir().unwrap();
        let missing_model = directory.path().join("missing-model.bin");
        ATTEMPT_BACKEND
            .scope(std::cell::Cell::new(Some("sidecar")), async {
                tokio::spawn(async move {
                    assert_eq!(attempt_backend(), None);
                    assert!(Transcriber::new(&missing_model).is_err());
                    assert_eq!(attempt_backend(), None);
                })
                .await
                .unwrap();
                assert_eq!(attempt_backend(), Some("sidecar"));
            })
            .await;
    }

    #[tokio::test]
    async fn concurrent_scoped_attempts_have_isolated_tags() {
        // Two overlapping transcription attempts must never observe each
        // other's backend: the tag is task-local state, not process-global.
        let first = tokio::spawn(ATTEMPT_BACKEND.scope(std::cell::Cell::new(None), async {
            record_attempt_backend("sidecar");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            attempt_backend()
        }));
        let second = tokio::spawn(ATTEMPT_BACKEND.scope(std::cell::Cell::new(None), async {
            record_attempt_backend("cpu");
            attempt_backend()
        }));

        assert_eq!(first.await.unwrap(), Some("sidecar"));
        assert_eq!(second.await.unwrap(), Some("cpu"));
    }

    #[tokio::test]
    async fn scoped_attempt_starts_empty_and_records_before_read() {
        // The scope starts empty; only an actual backend selection records it.
        let outcome = ATTEMPT_BACKEND
            .scope(std::cell::Cell::new(None), async {
                let before = attempt_backend();
                record_attempt_backend("metal");
                (before, attempt_backend())
            })
            .await;
        assert_eq!(outcome, (None, Some("metal")));
    }
}
