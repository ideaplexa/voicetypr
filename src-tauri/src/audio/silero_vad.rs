//! Content-free, advisory analysis only. Nothing here returns an engine decision.
//! The recording adapter starts this work only after the transcription attempt ends.
use std::io::{Read, Seek};
use std::time::Duration;
use std::time::Instant;

use serde::Serialize;

pub const MODEL_SHA256: &str = "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987";
pub const BUDGET: Duration = Duration::from_millis(250);

#[derive(Debug, Serialize)]
pub struct Summary {
    pub status: &'static str,
    pub runtime_ms: u64,
    pub frames: u64,
    pub analyzed_ms: u64,
    pub speech_ms: u64,
    pub probability_mean_milli: u64,
    pub probability_max_milli: u64,
}

impl Summary {
    pub fn skipped(status: &'static str) -> Self {
        Self {
            status,
            runtime_ms: 0,
            frames: 0,
            analyzed_ms: 0,
            speech_ms: 0,
            probability_mean_milli: 0,
            probability_max_milli: 0,
        }
    }

    pub fn speech_detected(&self) -> Option<bool> {
        (self.status == "complete").then_some(self.speech_ms > 0)
    }
}

fn expired(start: Instant, budget: Duration) -> Result<(), &'static str> {
    if start.elapsed() >= budget {
        Err("budget")
    } else {
        Ok(())
    }
}

/// Matches the pinned non-native ggml x86 build, including OS-enabled AVX state.
fn cpu_supported() -> bool {
    #[cfg(test)]
    if FORCE_UNSUPPORTED.get() {
        return false;
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        std::is_x86_feature_detected!("avx")
            && std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("f16c")
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        true
    }
}

/// Install discard-only native log hooks at entry, before any native workers.
/// Installing global callbacks from a shadow worker would race Whisper logging.
pub(crate) fn initialize_logging() {
    if cpu_supported() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            // SAFETY: static, non-unwinding callback; never dereferences or emits text.
            unsafe { whisper_rs::set_log_callback(Some(native_log), std::ptr::null_mut()) };
        });
    }
}

// Match bindgen's platform-specific C enum representation, as whisper-rs does.
#[cfg(all(windows, not(target_env = "gnu")))]
type NativeLogLevel = i32;
#[cfg(not(all(windows, not(target_env = "gnu"))))]
type NativeLogLevel = u32;

thread_local! {
    static NATIVE_FAILED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn observe_native_log(level: whisper_rs::GGMLLogLevel) {
    if matches!(level, whisper_rs::GGMLLogLevel::Error) {
        // A callback during thread-local destruction must not unwind across C.
        let _ = NATIVE_FAILED.try_with(|failed| failed.set(true));
    }
}

unsafe extern "C" fn native_log(
    level: NativeLogLevel,
    _text: *const std::ffi::c_char,
    _data: *mut std::ffi::c_void,
) {
    observe_native_log(level.into());
}

#[cfg(test)]
thread_local! {
    static FORCE_UNSUPPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn without_cpu<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            FORCE_UNSUPPORTED.set(self.0);
        }
    }
    let _reset = Reset(FORCE_UNSUPPORTED.replace(true));
    f()
}

pub fn analyze(input: impl Read + Seek, budget: Duration) -> Summary {
    if budget.is_zero() {
        return Summary::skipped("budget");
    }
    if !cpu_supported() {
        return Summary::skipped("runtime_unsupported");
    }
    runtime::analyze(input, budget)
}

#[cfg(test)]
mod cpu_tests {
    #[test]
    fn unsupported_cpu_skips_before_io_or_native_call() {
        struct NoIo;
        impl std::io::Read for NoIo {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("read");
            }
        }
        impl std::io::Seek for NoIo {
            fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
                panic!("seek");
            }
        }
        super::without_cpu(|| {
            let summary = super::analyze(NoIo, super::BUDGET);
            assert_eq!(summary.status, "runtime_unsupported");
            assert_eq!(summary.speech_detected(), None);
        });
    }
}

mod runtime {
    use super::*;
    use crate::audio::resampler::StreamingResampler;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use whisper_rs::{WhisperVadContext, WhisperVadContextParams};
    const MODEL: &[u8] = include_bytes!("../../models/silero/ggml-silero-v6.2.0.bin");
    const MAX_SECONDS: u32 = 120;
    const RATE: usize = 16_000;
    const WINDOW: usize = 512;

    /// Streaming decode/downmix/resample, without normalization, trimming or timestamps
    /// rebased to a speech boundary. Memory is capped at 120 seconds of mono 16 kHz.
    pub(super) fn read_wav(
        input: impl Read + Seek,
        start: Instant,
        budget: Duration,
    ) -> Result<Vec<f32>, &'static str> {
        expired(start, budget)?;
        let mut reader = hound::WavReader::new(input).map_err(|_| "unavailable")?;
        let spec = reader.spec();
        if spec.sample_format != hound::SampleFormat::Int
            || spec.bits_per_sample != 16
            || !(1..=2).contains(&spec.channels)
            || !(8_000..=96_000).contains(&spec.sample_rate)
        {
            return Err("unsupported");
        }
        let duration = reader.duration();
        if duration == 0 {
            return Err("empty");
        }
        if duration > spec.sample_rate * MAX_SECONDS {
            return Err("too_long");
        }
        let mut resampler = if spec.sample_rate == RATE as u32 {
            None
        } else {
            Some(StreamingResampler::new(spec.sample_rate as usize).map_err(|_| "unavailable")?)
        };
        let mut samples = reader.samples::<i16>();
        let mut output = Vec::with_capacity(
            ((u64::from(duration) * RATE as u64).div_ceil(u64::from(spec.sample_rate))) as usize,
        );
        let mut chunk = Vec::with_capacity(2048);
        loop {
            expired(start, budget)?;
            chunk.clear();
            for _ in 0..2048 {
                let Some(first) = samples.next() else { break };
                let mut sum = f32::from(first.map_err(|_| "unavailable")?) / 32768.0;
                if spec.channels == 2 {
                    let second = samples
                        .next()
                        .ok_or("unavailable")?
                        .map_err(|_| "unavailable")?;
                    sum = (sum + f32::from(second) / 32768.0) * 0.5;
                }
                chunk.push(sum);
            }
            if chunk.is_empty() {
                break;
            }
            if let Some(resampler) = resampler.as_mut() {
                output.extend(resampler.push(&chunk).map_err(|_| "unavailable")?);
            } else {
                output.extend_from_slice(&chunk);
            }
            if output.len() > RATE * MAX_SECONDS as usize {
                return Err("too_long");
            }
        }
        if let Some(resampler) = resampler {
            output.extend(resampler.finish().map_err(|_| "unavailable")?);
        }
        expired(start, budget)?;
        Ok(output)
    }

    /// One CPU thread and a fresh context per take. The native API processes the
    /// whole take without a cancellation callback. Deadline checks are cooperative;
    /// over-budget work remains unknown and holds the single worker until it exits.
    pub fn analyze(input: impl Read + Seek, budget: Duration) -> Summary {
        NATIVE_FAILED.set(false);
        let start = Instant::now();
        let mut result = (|| {
            let samples = read_wav(input, start, budget)?;
            if samples.is_empty() {
                return Err("empty");
            }
            if hex::encode(Sha256::digest(MODEL)) != MODEL_SHA256 {
                return Err("model_invalid");
            }
            expired(start, budget)?;
            let mut model = tempfile::NamedTempFile::new().map_err(|_| "unavailable")?;
            model.write_all(MODEL).map_err(|_| "unavailable")?;
            let path = model.path().to_str().ok_or("unavailable")?;
            let mut params = WhisperVadContextParams::new();
            params.set_use_gpu(false);
            params.set_n_threads(1);
            let mut context = WhisperVadContext::new(path, params).map_err(|_| "unavailable")?;
            expired(start, budget)?;
            context.detect_speech(&samples).map_err(|_| "unavailable")?;
            expired(start, budget)?;
            summarize(&samples, context.probabilities())
        })();
        // Include native context destruction and temporary-file cleanup in the budget.
        if start.elapsed() >= budget {
            result = Err("budget");
        }
        let mut summary = result.unwrap_or_else(Summary::skipped);
        summary.runtime_ms = start.elapsed().as_millis().min(86_400_000) as u64;
        summary
    }

    pub(super) fn summarize(
        samples: &[f32],
        probabilities: &[f32],
    ) -> Result<Summary, &'static str> {
        // Native frame compute failure logs on the caller thread but returns
        // true with zero-filled unfinished probabilities in the pinned API.
        if NATIVE_FAILED.get() {
            return Err("unavailable");
        }
        if samples.is_empty() {
            return Err("empty");
        }
        if probabilities.len() != samples.len().div_ceil(WINDOW)
            || probabilities
                .iter()
                .any(|p| !p.is_finite() || !(0.0..=1.0).contains(p))
        {
            return Err("unavailable");
        }
        let mut summary = Summary::skipped("complete");
        let mut total_probability = 0.0_f64;
        let mut speech_samples = 0_usize;
        for (chunk, &probability) in samples.chunks(WINDOW).zip(probabilities) {
            summary.frames += 1;
            total_probability += f64::from(probability);
            summary.probability_max_milli = summary
                .probability_max_milli
                .max((probability * 1000.0).round() as u64);
            // No minimum-duration filtering, padding, trim or segmentation.
            if probability >= 0.5 {
                speech_samples += chunk.len();
            }
        }
        summary.analyzed_ms = (samples.len() * 1000 / RATE) as u64;
        summary.speech_ms = (speech_samples * 1000).div_ceil(RATE) as u64;
        summary.probability_mean_milli =
            (total_probability * 1000.0 / summary.frames.max(1) as f64).round() as u64;
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_frame_errors_cannot_turn_zero_filled_probabilities_into_silence() {
        NATIVE_FAILED.set(false);
        observe_native_log(whisper_rs::GGMLLogLevel::Info);
        assert!(runtime::summarize(&[0.0; 512], &[0.0]).is_ok());
        observe_native_log(whisper_rs::GGMLLogLevel::Error);
        assert_eq!(
            runtime::summarize(&[0.0; 512], &[0.0]).err(),
            Some("unavailable")
        );
        NATIVE_FAILED.set(false);
    }

    #[test]
    fn probability_threshold_and_partial_tail_count_only_actual_samples() {
        let samples = vec![0.0_f32; 513];
        let summary = runtime::summarize(&samples, &[0.0, 1.0]).unwrap();
        assert_eq!(summary.frames, 2);
        assert_eq!(summary.analyzed_ms, 32);
        assert_eq!(summary.speech_ms, 1);
        assert_eq!(summary.probability_mean_milli, 500);
        assert_eq!(summary.probability_max_milli, 1000);
        let summary = runtime::summarize(&samples, &[0.5, 0.49]).unwrap();
        assert_eq!(summary.speech_ms, 32);
        for probabilities in [
            vec![0.0],
            vec![0.0, f32::NAN],
            vec![0.0, f32::INFINITY],
            vec![0.0, -0.01],
            vec![0.0, 1.01],
        ] {
            assert_eq!(
                runtime::summarize(&samples, &probabilities).err(),
                Some("unavailable")
            );
        }
        assert_eq!(runtime::summarize(&[], &[]).err(), Some("empty"));
    }

    #[test]
    fn bundled_ggml_bytes_and_provenance_match_the_runtime_pin() {
        use sha2::{Digest, Sha256};
        let model = include_bytes!("../../models/silero/ggml-silero-v6.2.0.bin");
        let provenance: serde_json::Value =
            serde_json::from_str(include_str!("../../models/silero/provenance.json")).unwrap();
        assert_eq!(model.len(), 885_098);
        assert_eq!(hex::encode(Sha256::digest(model)), MODEL_SHA256);
        assert_eq!(provenance["sha256"], MODEL_SHA256);
        assert_eq!(provenance["size_bytes"], model.len());
    }

    #[test]
    fn expired_budget_never_reads_audio_or_loads_runtime() {
        struct NoRead;
        impl Read for NoRead {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                panic!("expired work must not read");
            }
        }
        impl Seek for NoRead {
            fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
                panic!("expired work must not seek");
            }
        }
        let summary = analyze(NoRead, Duration::ZERO);
        assert_eq!(summary.status, "budget");
        assert_eq!(summary.speech_detected(), None);
        assert_eq!(summary.frames, 0);
    }

    #[test]
    fn incomplete_results_never_claim_silence() {
        for status in [
            "budget",
            "busy",
            "unavailable",
            "too_long",
            "unsupported",
            "runtime_unsupported",
            "empty",
        ] {
            assert_eq!(Summary::skipped(status).speech_detected(), None);
        }
    }

    fn wav(samples: &[i16], rate: u32, channels: u16) -> std::io::Cursor<Vec<u8>> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(
                &mut bytes,
                hound::WavSpec {
                    channels,
                    sample_rate: rate,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for sample in samples {
                writer.write_sample(*sample).unwrap();
            }
            writer.finalize().unwrap();
        }
        bytes.set_position(0);
        bytes
    }

    #[test]
    fn raw_audio_keeps_leading_silence_and_partial_tail() {
        let mut samples = vec![0_i16; 16_001];
        samples[16_000] = 4000;
        let input = wav(&samples, 16_000, 1);
        let decoded = runtime::read_wav(input, Instant::now(), Duration::from_secs(10)).unwrap();
        assert_eq!(decoded.len(), 16_001);
        assert_eq!(decoded[16_000], 4000.0 / 32768.0);
        assert!(decoded[..16_000].iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn bundled_model_loads_and_recurrent_state_is_reset_between_takes() {
        initialize_logging();
        let silence = vec![0_i16; 16_001];
        let first = analyze(wav(&silence, 16_000, 1), Duration::from_secs(10));
        if !cpu_supported() {
            assert_eq!(first.status, "runtime_unsupported");
            return;
        }
        let noise = (0..16_000)
            .map(|i| ((i * 7919 % 2000) - 1000) as i16)
            .collect::<Vec<_>>();
        let middle = analyze(wav(&noise, 16_000, 1), Duration::from_secs(10));
        let last = analyze(wav(&silence, 16_000, 1), Duration::from_secs(10));
        assert_eq!(first.status, "complete");
        assert_eq!(middle.status, "complete");
        assert_eq!(last.status, "complete");
        assert_eq!(first.frames, 32);
        assert_eq!(first.probability_mean_milli, last.probability_mean_milli);
        assert_eq!(first.probability_max_milli, last.probability_max_milli);
        assert_eq!(first.speech_detected(), Some(false));
        assert_eq!(last.speech_detected(), Some(false));
    }
}
