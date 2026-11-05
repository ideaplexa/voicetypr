//! Content-free, advisory analysis only. Nothing here returns an engine decision.
//! The recording adapter starts this work only after the transcription attempt ends.
use std::io::{Read, Seek};
use std::time::Duration;
#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
use std::time::Instant;

use serde::Serialize;

pub const MODEL_SHA256: &str = "7ed98ddbad84ccac4cd0aeb3099049280713df825c610a8ed34543318f1b2c49";
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

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
fn expired(start: Instant, budget: Duration) -> Result<(), &'static str> {
    if start.elapsed() >= budget {
        Err("budget")
    } else {
        Ok(())
    }
}

pub fn analyze(input: impl Read + Seek, budget: Duration) -> Summary {
    if budget.is_zero() {
        return Summary::skipped("budget");
    }
    if crate::onnx_runtime::require_cpu().is_err() {
        return Summary::skipped("runtime_unsupported");
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let _ = input;
        Summary::skipped("runtime_unsupported")
    }
    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    runtime::analyze(input, budget)
}

#[cfg(test)]
mod cpu_tests {
    #[test]
    fn unsupported_cpu_skips_before_io_or_any_ort_call() {
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
        crate::onnx_runtime::without_cpu(|| {
            let summary = super::analyze(NoIo, super::BUDGET);
            assert_eq!(summary.status, "runtime_unsupported");
            assert_eq!(summary.speech_detected(), None);
        });
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
mod runtime {
    use super::*;
    use crate::audio::resampler::StreamingResampler;
    use ort::{session::Session, value::Tensor};
    use sha2::{Digest, Sha256};
    const MODEL: &[u8] = include_bytes!("../../models/silero/silero_vad_16k_op15.onnx");
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

    /// One CPU thread, fresh recurrent state and 64-sample context for every take.
    /// The deadline is checked around each 32 ms frame. Native calls are cooperative,
    /// not hard-preemptible; the detached adapter retains its single slot until exit.
    pub fn analyze(input: impl Read + Seek, budget: Duration) -> Summary {
        let start = Instant::now();
        let result = (|| {
            let samples = read_wav(input, start, budget)?;
            if hex::encode(Sha256::digest(MODEL)) != MODEL_SHA256 {
                return Err("model_invalid");
            }
            expired(start, budget)?;
            let mut session = Session::builder()
                .map_err(|_| "unavailable")?
                .with_intra_threads(1)
                .map_err(|_| "unavailable")?
                .with_inter_threads(1)
                .map_err(|_| "unavailable")?
                .with_parallel_execution(false)
                .map_err(|_| "unavailable")?
                .commit_from_memory(MODEL)
                .map_err(|_| "unavailable")?;
            expired(start, budget)?;
            let mut state = vec![0.0_f32; 2 * 128];
            let mut context = [0.0_f32; 64];
            let mut summary = Summary::skipped("complete");
            let mut total_probability = 0.0_f64;
            let mut speech_samples = 0_usize;
            for chunk in samples.chunks(WINDOW) {
                expired(start, budget)?;
                let mut frame = vec![0.0_f32; 64 + WINDOW];
                frame[..64].copy_from_slice(&context);
                frame[64..64 + chunk.len()].copy_from_slice(chunk);
                context.copy_from_slice(&frame[WINDOW..]);
                let input = Tensor::from_array(([1_usize, 64 + WINDOW], frame))
                    .map_err(|_| "unavailable")?;
                let recurrent = Tensor::from_array(([2_usize, 1, 128], state.clone()))
                    .map_err(|_| "unavailable")?;
                let rate = Tensor::from_array(([0_usize; 0], vec![RATE as i64]))
                    .map_err(|_| "unavailable")?;
                let outputs = session
                    .run(ort::inputs!["input" => input, "state" => recurrent, "sr" => rate])
                    .map_err(|_| "unavailable")?;
                expired(start, budget)?;
                let (_, probabilities) = outputs["output"]
                    .try_extract_tensor::<f32>()
                    .map_err(|_| "unavailable")?;
                let probability = *probabilities.first().ok_or("unavailable")?;
                if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
                    return Err("unavailable");
                }
                let (_, next_state) = outputs["stateN"]
                    .try_extract_tensor::<f32>()
                    .map_err(|_| "unavailable")?;
                if next_state.len() != state.len() {
                    return Err("unavailable");
                }
                state.copy_from_slice(next_state);
                summary.frames += 1;
                total_probability += f64::from(probability);
                summary.probability_max_milli = summary
                    .probability_max_milli
                    .max((probability * 1000.0).round() as u64);
                // Advisory threshold only. No minimum speech duration discards short words.
                if probability >= 0.5 {
                    speech_samples += chunk.len();
                }
            }
            summary.analyzed_ms = (samples.len() * 1000 / RATE) as u64;
            summary.speech_ms = (speech_samples * 1000).div_ceil(RATE) as u64;
            summary.probability_mean_milli =
                (total_probability * 1000.0 / summary.frames.max(1) as f64).round() as u64;
            Ok(summary)
        })();
        let mut summary = result.unwrap_or_else(Summary::skipped);
        summary.runtime_ms = start.elapsed().as_millis().min(86_400_000) as u64;
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
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
    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
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
    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    fn bundled_model_loads_and_recurrent_state_is_reset_between_takes() {
        let silence = vec![0_i16; 16_001];
        let first = analyze(wav(&silence, 16_000, 1), Duration::from_secs(10));
        if crate::onnx_runtime::require_cpu().is_err() {
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
