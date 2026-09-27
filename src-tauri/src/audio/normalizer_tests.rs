use super::normalizer::{
    normalize_to_whisper_wav, normalize_to_whisper_wav_with_metrics, peak_normalization_gain,
};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::f32::consts::PI;
use std::fs;
use std::path::{Path, PathBuf};

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("voicetypr_test_{}", name))
}

fn write_sine_wav(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    secs: f32,
    amp: f32,
    freq: f32,
    silent_channels: &[u16],
) {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav");
    let total_frames = (secs * sample_rate as f32) as usize;

    for n in 0..total_frames {
        let t = n as f32 / sample_rate as f32;
        let sample = (amp * (2.0 * PI * freq * t).sin()).clamp(-1.0, 1.0);
        for ch in 0..channels {
            let s = if silent_channels.contains(&ch) {
                0.0
            } else {
                sample
            };
            let i = (s * 32767.0) as i16;
            writer.write_sample(i).expect("write sample");
        }
    }
    writer.finalize().expect("finalize wav");
}

fn write_noise_wav(path: &Path, sample_rate: u32, secs: f32, amp: f32) {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav");
    let total_frames = (secs * sample_rate as f32) as usize;
    let mut state = 0x1234_5678u32;

    for _ in 0..total_frames {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let unit = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let sample = (unit * amp).clamp(-1.0, 1.0);
        writer
            .write_sample((sample * 32767.0) as i16)
            .expect("write sample");
    }

    writer.finalize().expect("finalize wav");
}

/// Mono tone bursts separated by true silence — a syllabic envelope so the
/// modulation gate sees a wide loud-vs-quiet swing (like real quiet speech),
/// unlike a continuous tone or steady noise.
fn write_gapped_tone_wav(
    path: &Path,
    sample_rate: u32,
    secs: f32,
    amp: f32,
    freq: f32,
    on_ms: u32,
    off_ms: u32,
) {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav");
    let total_frames = (secs * sample_rate as f32) as usize;
    let on_frames = (on_ms * sample_rate / 1000) as usize;
    let off_frames = (off_ms * sample_rate / 1000) as usize;
    let cycle = (on_frames + off_frames).max(1);

    for n in 0..total_frames {
        let sample = if (n % cycle) < on_frames {
            let t = n as f32 / sample_rate as f32;
            (amp * (2.0 * PI * freq * t).sin()).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        writer
            .write_sample((sample * 32767.0) as i16)
            .expect("write sample");
    }
    writer.finalize().expect("finalize wav");
}

fn write_speech_then_silence_wav(
    path: &Path,
    sample_rate: u32,
    speech_secs: f32,
    silence_secs: f32,
    amp: f32,
    freq: f32,
) {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav");
    let speech_frames = (speech_secs * sample_rate as f32) as usize;
    let silence_frames = (silence_secs * sample_rate as f32) as usize;

    for n in 0..speech_frames {
        let t = n as f32 / sample_rate as f32;
        let sample = (amp * (2.0 * PI * freq * t).sin()).clamp(-1.0, 1.0);
        writer
            .write_sample((sample * 32767.0) as i16)
            .expect("write sample");
    }
    for _ in 0..silence_frames {
        writer.write_sample(0).expect("write sample");
    }
    writer.finalize().expect("finalize wav");
}

fn write_silence_wav(path: &Path, sample_rate: u32, secs: f32) {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec).expect("create wav");
    let total_frames = (secs * sample_rate as f32) as usize;
    for _ in 0..total_frames {
        writer.write_sample(0).expect("write sample");
    }
    writer.finalize().expect("finalize wav");
}

fn wav_duration_secs(path: &Path) -> f32 {
    let reader = hound::WavReader::open(path).expect("open wav");
    let spec = reader.spec();
    reader.duration() as f32 / spec.sample_rate as f32
}

fn read_peak(path: &Path) -> f32 {
    let samples: Vec<i16> = hound::WavReader::open(path)
        .expect("open wav")
        .samples::<i16>()
        .map(|s| s.unwrap())
        .collect();
    samples
        .iter()
        .map(|&sample| i32::from(sample).abs())
        .max()
        .unwrap_or(0) as f32
        / i16::MAX as f32
}

#[test]
fn speech_gated_gain_boosts_soft_gapped_speech_beyond_previous_cap() {
    let input = temp_file("soft_speech_in.wav");
    let out_dir = temp_file("soft_speech_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // Soft, syllabic (gapped) tone ~ real quiet speech: voiced bursts + silent gaps.
    write_gapped_tone_wav(&input, 16_000, 1.2, 0.03, 220.0, 200, 120);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");
    let peak = read_peak(&out_path);

    assert!(
        peak > 0.65 && peak <= 0.85,
        "soft gapped speech should reach the target peak instead of the previous ~0.30 cap; got {peak}"
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn speech_gated_gain_keeps_steady_moderate_noise_capped() {
    let input = temp_file("steady_noise_in.wav");
    let out_dir = temp_file("steady_noise_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // Moderate steady ambient (~fan/HVAC): RMS above the voice floor but a flat
    // envelope. Must stay at the historical 10x cap, not the 32x quiet-speech cap.
    write_noise_wav(&input, 16_000, 0.6, 0.012);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");
    let peak = read_peak(&out_path);

    assert!(
        peak <= 0.13,
        "steady moderate noise must stay near the 10x cap (~0.12), not be speech-boosted; got {peak}"
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn speech_gated_gain_keeps_steady_tone_capped() {
    let input = temp_file("steady_tone_in.wav");
    let out_dir = temp_file("steady_tone_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // A continuous tone has voiced energy but a flat envelope -> not speech-like.
    write_sine_wav(&input, 16_000, 1, 0.5, 0.03, 220.0, &[]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");
    let peak = read_peak(&out_path);

    assert!(
        peak <= 0.33,
        "a steady tone must stay near the 10x cap (~0.30), not be speech-boosted; got {peak}"
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn speech_gated_gain_keeps_near_silent_noise_at_previous_cap() {
    let input = temp_file("near_silent_noise_in.wav");
    let out_dir = temp_file("near_silent_noise_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    write_noise_wav(&input, 16_000, 0.5, 0.0004);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");
    let peak = read_peak(&out_path);

    assert!(
        peak <= 0.006,
        "near-silent noise should stay near the historical 10x cap, not be speech-boosted; got {peak}"
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn speech_gated_gain_leaves_normal_loud_peak_limited_clips_unchanged() {
    assert_eq!(peak_normalization_gain(0.5, false), 1.6);
    assert_eq!(peak_normalization_gain(0.5, true), 1.6);
    assert_eq!(peak_normalization_gain(0.08, false), 10.0);
    assert_eq!(peak_normalization_gain(0.08, true), 10.0);
}

#[test]
fn normalize_fails_on_missing_input() {
    let missing = temp_file("missing_input.wav");
    // Ensure it does not exist
    let _ = fs::remove_file(&missing);
    let out_dir = std::env::temp_dir();
    let err = normalize_to_whisper_wav(&missing, &out_dir).unwrap_err();
    assert!(err.contains("Input WAV does not exist"));
}

#[test]
fn normalize_basic_16k_mono_peak_and_format() {
    let input = temp_file("basic_16k_mono_in.wav");
    let out_dir = temp_file("basic_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // 0.25s of 1kHz sine at 0.5 amplitude
    write_sine_wav(&input, 16_000, 1, 0.25, 0.5, 1000.0, &[]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");

    // Read back and validate format
    let mut reader = hound::WavReader::open(&out_path).expect("open normalized");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 16);
    assert_eq!(spec.sample_format, SampleFormat::Int);

    // Check approximate peak around target (0.8) with tolerance due to dither/quantization
    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
    assert!(!samples.is_empty());
    let max = samples.iter().map(|s| s.abs() as i32).max().unwrap() as f32;
    let peak = max / i16::MAX as f32;
    // Allow generous tolerance (±0.1)
    assert!(
        peak > 0.65 && peak <= 0.85,
        "peak out of expected range: {}",
        peak
    );

    // Cleanup
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalize_resamples_48k_to_16k() {
    let input = temp_file("resample_48k_in.wav");
    let out_dir = temp_file("resample_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // 0.3s at 48kHz, mono
    write_sine_wav(&input, 48_000, 1, 0.3, 0.4, 800.0, &[]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");

    let reader = hound::WavReader::open(&out_path).expect("open normalized");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);

    // Duration should be roughly preserved (0.3s)
    let frames = reader.duration();
    let duration = frames as f32 / spec.sample_rate as f32;
    assert!(
        (duration - 0.3).abs() < 0.05,
        "duration {}s not ~0.3s",
        duration
    );

    // Cleanup
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalize_downmix_ignores_silent_channel() {
    let input = temp_file("downmix_stereo_in.wav");
    let out_dir = temp_file("downmix_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // Stereo: ch0 is 0.5 sine, ch1 is silent
    write_sine_wav(&input, 16_000, 2, 0.25, 0.5, 500.0, &[1]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");

    // Ensure output is mono 16k and non-silent
    let samples: Vec<i16> = hound::WavReader::open(&out_path)
        .expect("open out")
        .samples::<i16>()
        .map(|s| s.unwrap())
        .collect();
    assert!(!samples.is_empty());
    let max = samples.iter().map(|s| s.abs() as i32).max().unwrap();
    assert!(max > 0, "output should not be silent");

    // Cleanup
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalize_resamples_44100_to_16k_preserves_duration() {
    let input = temp_file("resample_44100_in.wav");
    let out_dir = temp_file("resample_44100_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // 1 second of 1 kHz sine at 44.1 kHz, mono
    write_sine_wav(&input, 44_100, 1, 1.0, 0.5, 1000.0, &[]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize 44.1kHz");

    let reader = hound::WavReader::open(&out_path).expect("open normalized");
    let spec = reader.spec();
    // Executor skip criteria: 16000 Hz / mono / 16-bit / Int
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 16);
    assert_eq!(spec.sample_format, SampleFormat::Int);

    // Duration should be roughly preserved (1.0 s input → ~1.0 s output, ±0.05 s tolerance)
    let frames = reader.duration();
    let duration = frames as f32 / spec.sample_rate as f32;
    assert!(
        (duration - 1.0).abs() < 0.05,
        "duration {}s not ~1.0s after 44.1kHz→16kHz resample",
        duration
    );

    // Cleanup
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalize_very_short_non_empty_wav_outputs_valid_16k_mono_s16() {
    let input = temp_file("very_short_in.wav");
    let out_dir = temp_file("very_short_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // 0.1 s (well below the 0.5 s duration gate) — the normalizer must still succeed;
    // the duration gate that rejects short clips operates AFTER normalization, not inside it.
    write_sine_wav(&input, 16_000, 1, 0.1, 0.6, 440.0, &[]);

    let out_path = normalize_to_whisper_wav(&input, &out_dir)
        .expect("normalizer must succeed even on very short input");

    let reader = hound::WavReader::open(&out_path).expect("open normalized");
    let spec = reader.spec();
    // Must satisfy all executor skip criteria regardless of duration
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 16);
    assert_eq!(spec.sample_format, SampleFormat::Int);

    // Must contain non-empty audio
    let samples: Vec<i16> = hound::WavReader::open(&out_path)
        .expect("re-open normalized")
        .samples::<i16>()
        .map(|s| s.unwrap())
        .collect();
    assert!(!samples.is_empty(), "output must be non-empty");

    // Cleanup
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn trims_long_trailing_digital_silence_after_speech() {
    let input = temp_file("trim_long_tail_in.wav");
    let out_dir = temp_file("trim_long_tail_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // 1.5s speech + 2s digital silence (16 kHz mono).
    write_speech_then_silence_wav(&input, 16_000, 1.5, 2.0, 0.5, 440.0);
    let input_duration = wav_duration_secs(&input);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");
    let out_duration = wav_duration_secs(&out_path);

    let removed = input_duration - out_duration;
    // Expect ~2s silence removed minus ~300ms retained context; allow windowing tolerance.
    assert!(
        removed > 1.5 && removed < 2.1,
        "expected ~1.7s removed, got removed={removed}s (in={input_duration}s out={out_duration}s)"
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn does_not_trim_short_pause() {
    let input = temp_file("trim_short_pause_in.wav");
    let out_dir = temp_file("trim_short_pause_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    write_speech_then_silence_wav(&input, 16_000, 1.0, 0.3, 0.5, 440.0);
    let input_duration = wav_duration_secs(&input);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");
    let out_duration = wav_duration_secs(&out_path);

    assert!(
        (out_duration - input_duration).abs() < 0.08,
        "300ms trailing pause should be kept: in={input_duration}s out={out_duration}s"
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn does_not_trim_without_sustained_voice() {
    let input = temp_file("trim_all_silence_in.wav");
    let out_dir = temp_file("trim_all_silence_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    write_silence_wav(&input, 16_000, 2.0);
    let input_duration = wav_duration_secs(&input);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");
    let out_duration = wav_duration_secs(&out_path);

    assert!(
        (out_duration - input_duration).abs() < 0.05,
        "all-silence clip must not be rewritten: in={input_duration}s out={out_duration}s"
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn does_not_trim_below_min_duration() {
    let input = temp_file("trim_min_duration_in.wav");
    let out_dir = temp_file("trim_min_duration_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    write_speech_then_silence_wav(&input, 16_000, 0.2, 0.5, 0.5, 440.0);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");

    let mut reader = hound::WavReader::open(&out_path).expect("open normalized");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 16_000);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 16);
    assert_eq!(spec.sample_format, SampleFormat::Int);

    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
    assert!(!samples.is_empty(), "very short clip must stay non-empty");

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn preserves_tail_above_silence_threshold() {
    let input = temp_file("trim_noisy_tail_in.wav");
    let out_dir = temp_file("trim_noisy_tail_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    // Speech then a low-level noise tail above TRAILING_SILENCE_RMS_THRESHOLD (0.00125).
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&input, spec).expect("create wav");
    let speech_frames = (1.0 * 16_000.0) as usize;
    let tail_frames = (2.0 * 16_000.0) as usize;
    let tail_amp = 0.02f32;
    let mut state = 0xDEAD_BEEFu32;

    for n in 0..speech_frames {
        let t = n as f32 / 16_000.0;
        let sample = (0.5 * (2.0 * PI * 440.0 * t).sin()).clamp(-1.0, 1.0);
        writer
            .write_sample((sample * 32767.0) as i16)
            .expect("write sample");
    }
    for _ in 0..tail_frames {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let unit = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let sample = (unit * tail_amp).clamp(-1.0, 1.0);
        writer
            .write_sample((sample * 32767.0) as i16)
            .expect("write sample");
    }
    writer.finalize().expect("finalize wav");
    let input_duration = wav_duration_secs(&input);

    let out_path = normalize_to_whisper_wav(&input, &out_dir).expect("normalize");
    let out_duration = wav_duration_secs(&out_path);

    assert!(
        (out_duration - input_duration).abs() < 0.15,
        "voiced/noisy tail above silence threshold should be kept: in={input_duration}s out={out_duration}s"
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&out_path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalization_metrics_reuse_gapped_speech_decision_and_gain() {
    let input = temp_file("metrics_gapped_speech_in.wav");
    let out_dir = temp_file("metrics_gapped_speech_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    write_gapped_tone_wav(&input, 16_000, 1.2, 0.03, 220.0, 200, 120);

    let normalized =
        normalize_to_whisper_wav_with_metrics(&input, &out_dir).expect("normalize with metrics");

    assert!(normalized.metrics.speech_like_modulation);
    assert!(normalized.metrics.pre_gain_peak > 0.02);
    assert!(normalized.metrics.applied_gain > 10.0);
    assert_eq!(normalized.metrics.input_duration_ms, 1200);
    assert_eq!(normalized.metrics.output_duration_ms, 1200);
    assert_eq!(normalized.metrics.trimmed_duration_ms, 0);

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&normalized.path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalization_metrics_report_digital_silence_without_inventing_speech() {
    let input = temp_file("metrics_silence_in.wav");
    let out_dir = temp_file("metrics_silence_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    write_silence_wav(&input, 16_000, 1.0);

    let normalized =
        normalize_to_whisper_wav_with_metrics(&input, &out_dir).expect("normalize with metrics");

    assert_eq!(normalized.metrics.pre_gain_peak, 0.0);
    assert!(!normalized.metrics.speech_like_modulation);
    assert_eq!(normalized.metrics.applied_gain, 1.0);
    assert_eq!(normalized.metrics.input_duration_ms, 1000);
    assert_eq!(normalized.metrics.output_duration_ms, 1000);
    assert_eq!(normalized.metrics.trimmed_duration_ms, 0);

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&normalized.path);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn normalization_metrics_account_for_trimmed_tail() {
    let input = temp_file("metrics_trim_in.wav");
    let out_dir = temp_file("metrics_trim_out_dir");
    let _ = fs::create_dir_all(&out_dir);
    write_speech_then_silence_wav(&input, 16_000, 1.0, 2.0, 0.5, 440.0);

    let normalized =
        normalize_to_whisper_wav_with_metrics(&input, &out_dir).expect("normalize with metrics");

    assert_eq!(normalized.metrics.input_duration_ms, 3000);
    assert!(normalized.metrics.trimmed_duration_ms >= 1500);
    assert_eq!(
        normalized.metrics.output_duration_ms + normalized.metrics.trimmed_duration_ms,
        normalized.metrics.input_duration_ms
    );

    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&normalized.path);
    let _ = fs::remove_dir_all(&out_dir);
}
