#![allow(dead_code)]

use crate::audio::resampler::resample_to_16khz;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use rand::Rng;
use std::fs;
use std::path::{Path, PathBuf};

const TARGET_RATE: u32 = 16_000;
const TARGET_CHANNELS: u16 = 1;
const TARGET_BITS: u16 = 16;
const TARGET_PEAK: f32 = 0.8; // ~ -1.9 dBFS
const STANDARD_MAX_GAIN: f32 = 10.0;
const SPEECH_MAX_GAIN: f32 = 32.0;
const SPEECH_RMS_FLOOR: f32 = crate::audio::silence_detector::VOICE_RMS_THRESHOLD;
const SILENCE_RMS_THRESHOLD: f32 = 1e-4; // ~ -80 dBFS
const TRIM_WINDOW_SAMPLES: usize = TARGET_RATE as usize / 50; // 20ms @16k = 320
const REQUIRED_VOICE_WINDOWS: usize = 15; // 300ms sustained voice
const TRAILING_SILENCE_RMS_THRESHOLD: f32 =
    crate::audio::silence_detector::VOICE_RMS_THRESHOLD * 0.25; // 0.00125
const MIN_TRAILING_SILENCE_TO_TRIM_WINDOWS: usize = 35; // 700ms
const RETAIN_TRAILING_CONTEXT_WINDOWS: usize = 15; // keep 300ms after last speech
const MIN_RETAINED_SAMPLES_AFTER_TRIM: usize = TARGET_RATE as usize / 2; // 0.5s floor

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizationMetrics {
    pub pre_gain_peak: f32,
    pub speech_like_modulation: bool,
    pub applied_gain: f32,
    pub input_duration_ms: u64,
    pub output_duration_ms: u64,
    pub trimmed_duration_ms: u64,
}

#[derive(Debug)]
pub struct NormalizedAudio {
    pub path: PathBuf,
    pub metrics: NormalizationMetrics,
}

fn duration_ms(sample_count: usize) -> u64 {
    (sample_count as u64)
        .saturating_mul(1000)
        .checked_div(u64::from(TARGET_RATE))
        .unwrap_or(0)
}

/// Normalize any WAV (our recorder output) to the local engine contract:
/// WAV PCM S16LE, mono, 16 kHz, peak-normalized with speech-gated quiet-clip gain and light dither.
pub fn normalize_to_whisper_wav(input_wav: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    normalize_to_whisper_wav_with_metrics(input_wav, out_dir).map(|audio| audio.path)
}

/// Normalize audio and return evidence already computed while preparing the
/// engine input. Metric collection adds no separate full-buffer traversal.
pub fn normalize_to_whisper_wav_with_metrics(
    input_wav: &Path,
    out_dir: &Path,
) -> Result<NormalizedAudio, String> {
    if !input_wav.exists() {
        return Err(format!("Input WAV does not exist: {:?}", input_wav));
    }

    fs::create_dir_all(out_dir).map_err(|e| format!("Failed to create out_dir: {}", e))?;

    // Open source wav (expect our recorder: PCM 16-bit interleaved)
    let mut reader =
        WavReader::open(input_wav).map_err(|e| format!("Failed to open WAV: {}", e))?;
    let spec = reader.spec();

    if spec.sample_format != SampleFormat::Int || spec.bits_per_sample != 16 {
        // For now we handle our own 16-bit files; other formats can be extended later.
        // Avoid surprising runtime errors by surfacing a clear message.
        log::warn!(
            "Normalizer expected 16-bit PCM. Got {:?} {}-bit; proceeding best-effort.",
            spec.sample_format,
            spec.bits_per_sample
        );
    }

    let channels = spec.channels.max(1);
    let sample_rate = spec.sample_rate.max(1);

    // Read samples as i16 → f32 [-1,1]
    let samples_i16: Vec<i16> = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read samples: {}", e))?;
    if samples_i16.is_empty() {
        return Err("WAV contains no samples".to_string());
    }

    let samples_f32: Vec<f32> = samples_i16
        .iter()
        .map(|&s| s as f32 / i16::MAX as f32)
        .collect();

    // If multi-channel, compute per-channel RMS and ignore near-silent channels.
    let mono: Vec<f32> = if channels == 1 {
        samples_f32
    } else {
        downmix_equal_power_ignore_silent(&samples_f32, channels as usize)
    };

    // Resample to 16 kHz using our high-quality rubato resampler
    let resampled = if sample_rate != TARGET_RATE {
        resample_to_16khz(&mono, sample_rate)?
    } else {
        mono
    };

    // Peak-normalize to TARGET_PEAK with the historical 10x safety cap. Only unlock
    // the larger quiet-speech cap when the clip is modulated like real speech
    // (voiced energy AND genuine near-silent gaps); steady noise/tones stay at 10x.
    let peak = resampled.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
    let speech_like = has_speech_like_modulation(&resampled);
    let input_duration_ms = duration_ms(resampled.len());
    let gain = peak_normalization_gain(peak, speech_like);
    let normalized: Vec<f32> = if (gain - 1.0).abs() > 1e-3 {
        resampled
            .iter()
            .map(|&x| (x * gain).clamp(-1.0, 1.0))
            .collect()
    } else {
        resampled
    };

    let trimmed = trim_trailing_silence_for_whisper(&normalized);
    let output_duration_ms = duration_ms(trimmed.len());
    let metrics = NormalizationMetrics {
        pre_gain_peak: peak,
        speech_like_modulation: speech_like,
        applied_gain: gain,
        input_duration_ms,
        output_duration_ms,
        trimmed_duration_ms: input_duration_ms.saturating_sub(output_duration_ms),
    };

    // Quantize to i16 with TPDF dither
    let mut rng = rand::thread_rng();
    let mut pcm_i16 = Vec::with_capacity(trimmed.len());
    for &x in trimmed {
        // TPDF dither: add two independent uniform(-0.5,0.5) LSBs
        let dither = (rng.gen::<f32>() - 0.5) + (rng.gen::<f32>() - 0.5);
        let y = (x * i16::MAX as f32 + dither).clamp(i16::MIN as f32, i16::MAX as f32);
        pcm_i16.push(y as i16);
    }

    // Write final WAV
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let out_path = out_dir.join(format!("normalized_{}.wav", ts));
    let out_spec = WavSpec {
        channels: TARGET_CHANNELS,
        sample_rate: TARGET_RATE,
        bits_per_sample: TARGET_BITS,
        sample_format: SampleFormat::Int,
    };
    let mut writer =
        WavWriter::create(&out_path, out_spec).map_err(|e| format!("WAV create failed: {}", e))?;
    for s in pcm_i16 {
        writer
            .write_sample(s)
            .map_err(|e| format!("WAV write failed: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalize failed: {}", e))?;

    Ok(NormalizedAudio {
        path: out_path,
        metrics,
    })
}

pub(crate) fn peak_normalization_gain(peak: f32, speech_like: bool) -> f32 {
    if peak <= 0.0 {
        return 1.0;
    }

    let max_gain = if speech_like {
        SPEECH_MAX_GAIN
    } else {
        STANDARD_MAX_GAIN
    };

    (TARGET_PEAK / peak).min(max_gain)
}

fn trim_trailing_silence_for_whisper(samples: &[f32]) -> &[f32] {
    if samples.len() <= MIN_RETAINED_SAMPLES_AFTER_TRIM {
        return samples;
    }

    let window_rms = collect_window_rms(samples);
    if !has_sustained_voice_windows(&window_rms) {
        return samples;
    }

    let silent_tail = trailing_silence_windows(&window_rms);
    if silent_tail < MIN_TRAILING_SILENCE_TO_TRIM_WINDOWS {
        return samples;
    }

    let total_windows = window_rms.len();
    let silent_tail_start = total_windows.saturating_sub(silent_tail);
    let cut_window = (silent_tail_start + RETAIN_TRAILING_CONTEXT_WINDOWS).min(total_windows);
    let cut_samples = (cut_window * TRIM_WINDOW_SAMPLES).min(samples.len());
    if cut_samples < MIN_RETAINED_SAMPLES_AFTER_TRIM {
        return samples;
    }

    let removed_samples = samples.len().saturating_sub(cut_samples);
    let kept_ms = cut_samples as f32 / TARGET_RATE as f32 * 1000.0;
    let removed_ms = removed_samples as f32 / TARGET_RATE as f32 * 1000.0;
    log::info!(
        "Trimmed trailing digital silence for Whisper: kept {:.0}ms, removed {:.0}ms",
        kept_ms,
        removed_ms
    );

    &samples[..cut_samples]
}

fn collect_window_rms(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks(TRIM_WINDOW_SAMPLES)
        .filter(|window| !window.is_empty())
        .map(|window| {
            let sum_sq: f32 = window.iter().map(|&x| x * x).sum();
            (sum_sq / window.len() as f32).sqrt()
        })
        .collect()
}

fn has_sustained_voice_windows(window_rms: &[f32]) -> bool {
    let voice_threshold = crate::audio::silence_detector::VOICE_RMS_THRESHOLD;
    let mut run = 0usize;
    for &rms in window_rms {
        if rms >= voice_threshold {
            run += 1;
            if run >= REQUIRED_VOICE_WINDOWS {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn trailing_silence_windows(window_rms: &[f32]) -> usize {
    window_rms
        .iter()
        .rev()
        .take_while(|&&rms| rms <= TRAILING_SILENCE_RMS_THRESHOLD)
        .count()
}

fn has_speech_like_modulation(samples: &[f32]) -> bool {
    const WINDOW_SAMPLES: usize = TARGET_RATE as usize / 50; // 20 ms at 16 kHz.
    const REQUIRED_VOICED_WINDOWS: usize = 15; // >= 300 ms of voiced energy.
    const MODULATION_RATIO: f32 = 4.0; // ~12 dB loud-vs-quiet envelope swing of speech.

    let mut window_rms: Vec<f32> = samples
        .chunks(WINDOW_SAMPLES)
        .filter(|window| !window.is_empty())
        .map(|window| {
            let sumsq: f32 = window.iter().map(|sample| sample * sample).sum();
            (sumsq / window.len() as f32).sqrt()
        })
        .collect();
    if window_rms.len() < REQUIRED_VOICED_WINDOWS {
        return false;
    }

    // Enough genuine voiced energy to be worth boosting.
    let voiced = window_rms
        .iter()
        .filter(|&&rms| rms >= SPEECH_RMS_FLOOR)
        .count();
    if voiced < REQUIRED_VOICED_WINDOWS {
        return false;
    }

    // Per-clip dynamic range: speech swings between loud (vowels) and quiet
    // (pauses / the clip's own noise floor); steady noise and pure tones have a
    // flat envelope. Comparing the clip's 90th- vs 10th-percentile window energy
    // keeps the boost alive on real (slightly noisy) mics while rejecting steady
    // signals regardless of their absolute level.
    window_rms.sort_by(f32::total_cmp);
    let floor = window_rms[window_rms.len() / 10].max(SILENCE_RMS_THRESHOLD);
    let loud = window_rms[window_rms.len() * 9 / 10];
    loud >= floor * MODULATION_RATIO
}

fn downmix_equal_power_ignore_silent(input: &[f32], channels: usize) -> Vec<f32> {
    if channels == 0 {
        return vec![];
    }
    let frames = input.len() / channels;
    if frames == 0 {
        return vec![];
    }

    // RMS per channel
    let mut sumsq = vec![0.0f32; channels];
    for frame in 0..frames {
        let base = frame * channels;
        for ch in 0..channels {
            let s = input[base + ch];
            sumsq[ch] += s * s;
        }
    }
    let rms: Vec<f32> = sumsq.iter().map(|&s| (s / frames as f32).sqrt()).collect();
    let mut active: Vec<usize> = rms
        .iter()
        .enumerate()
        .filter(|(_, &e)| e > SILENCE_RMS_THRESHOLD)
        .map(|(i, _)| i)
        .collect();
    if active.is_empty() {
        // If all channels are silent by threshold, use all channels to avoid empty output
        active = (0..channels).collect();
    }

    let gain = (1.0f32 / (active.len() as f32)).sqrt();

    let mut out = Vec::with_capacity(frames);
    for frame in 0..frames {
        let base = frame * channels;
        let mut sum = 0.0f32;
        for &ch in &active {
            sum += input[base + ch];
        }
        out.push((sum * gain).clamp(-1.0, 1.0));
    }
    out
}
