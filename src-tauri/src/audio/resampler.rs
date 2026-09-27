use audioadapter_buffers::direct::SequentialSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

/// Resample audio from any sample rate to 16kHz for Whisper.
///
/// Uses rubato's FFT-based synchronous resampler with `process_all_into_buffer`,
/// which handles arbitrary-length clips in a single call — including chunking,
/// partial tails, and internal delay buffer flushing. This eliminates tail-sample
/// loss that occurred with manual single-shot `process_into_buffer` calls.
pub fn resample_to_16khz(input: &[f32], input_sample_rate: u32) -> Result<Vec<f32>, String> {
    if input_sample_rate == 16_000 {
        log::debug!("Audio already at 16kHz, no resampling needed");
        return Ok(input.to_vec());
    }

    log::info!("Resampling audio from {} Hz to 16000 Hz", input_sample_rate);

    // Fft resampler: fast, always best quality for fixed-ratio offline resampling.
    // FixedSync::Both lets rubato pick optimal chunk sizes for the ratio.
    let mut resampler = Fft::<f32>::new(
        input_sample_rate as usize,
        16_000,
        1024, // Internal processing block size; process_all_into_buffer handles any input length
        1,    // sub_chunks
        1,    // mono
        FixedSync::Both,
    )
    .map_err(|e| format!("Failed to create resampler: {:?}", e))?;

    // Allocate output buffer sized by the resampler's own calculation.
    let out_len = resampler.process_all_needed_output_len(input.len());
    let mut output = vec![0.0f32; out_len];

    // Wrap input/output in audioadapter slices (1 channel = mono).
    let input_adapter = SequentialSlice::new(input, 1, input.len())
        .map_err(|e| format!("Failed to create input adapter: {:?}", e))?;
    let mut output_adapter = SequentialSlice::new_mut(&mut output, 1, out_len)
        .map_err(|e| format!("Failed to create output adapter: {:?}", e))?;

    // Single call handles chunking, partial tails, and delay buffer flush.
    let (_in_frames, out_frames) = resampler
        .process_all_into_buffer(&input_adapter, &mut output_adapter, input.len(), None)
        .map_err(|e| format!("Resampling failed: {:?}", e))?;

    output.truncate(out_frames);

    log::info!(
        "Resampled {} samples to {} samples (ratio: {:.4})",
        input.len(),
        output.len(),
        16_000_f64 / input_sample_rate as f64
    );

    Ok(output)
}

/// Streaming mono resampler (Phase 3): feeds fixed-size chunks to rubato's
/// synchronous `Fft` resampler incrementally, so peak memory stays bounded by a
/// single resampler chunk regardless of total input length. Used by
/// `decode::stream_*` to keep huge-file decode memory flat.
///
/// Output matches the batch `resample_to_16khz` within rounding: the resampler's
/// leading latency is trimmed off the front, and the EOF flush pads silence so
/// the final length is `ceil(ratio * input_len)`.
pub struct StreamingResampler {
    resampler: Fft<f32>,
    pending: Vec<f32>,
    /// Leading output latency still to drop before emitting real samples.
    frames_to_skip: usize,
    total_in: usize,
    total_emitted: usize,
}

impl StreamingResampler {
    /// Build a resampler for `input_rate -> 16 kHz`. `input_rate` must differ
    /// from 16 kHz; callers that already have 16 kHz audio should skip
    /// resampling entirely rather than construct this.
    pub fn new(input_rate: usize) -> Result<Self, String> {
        let resampler = Fft::<f32>::new(
            input_rate,
            16_000,
            1024, // processing block size; FixedSync::Both picks the real chunk
            1,    // sub_chunks
            1,    // mono
            FixedSync::Both,
        )
        .map_err(|e| format!("Failed to create streaming resampler: {:?}", e))?;
        let frames_to_skip = resampler.output_delay();
        Ok(Self {
            resampler,
            pending: Vec::with_capacity(2048),
            frames_to_skip,
            total_in: 0,
            total_emitted: 0,
        })
    }

    /// Feed input frames; returns whatever 16 kHz output is currently ready.
    /// Early calls may return nothing while the resampler's latency is flushed.
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        self.pending.extend_from_slice(samples);
        self.total_in += samples.len();
        let mut out = Vec::new();
        loop {
            let needed = self.resampler.input_frames_next();
            if self.pending.len() < needed {
                break;
            }
            let chunk: Vec<f32> = self.pending.drain(..needed).collect();
            let frames = self.process(&chunk, None)?;
            self.collect(&frames, &mut out);
        }
        Ok(out)
    }

    /// Flush at EOF. Any partial trailing input is zero-padded by rubato (via
    /// `partial_len`) and the delay line is pumped with silence until the
    /// ratio-accurate length is reached, then any rounding overshoot is clipped.
    pub fn finish(mut self) -> Result<Vec<f32>, String> {
        let mut out = Vec::new();
        if !self.pending.is_empty() {
            let remaining = self.pending.len();
            let chunk = std::mem::take(&mut self.pending);
            let frames = self.process(&chunk, Some(remaining))?;
            self.collect(&frames, &mut out);
        }
        let expected = (self.resampler.resample_ratio() * self.total_in as f64).ceil() as usize;
        // Pump full silence chunks through the delay line until the ratio-
        // accurate length is reached. Guarded so a pathological resampler can't
        // loop forever; feeding real zeros is equivalent to partial_len padding.
        let zero_chunk = vec![0f32; self.resampler.input_frames_next()];
        let mut guard = 0;
        while self.total_emitted < expected && guard < 4096 {
            guard += 1;
            let frames = self.process(&zero_chunk, None)?;
            if frames.is_empty() {
                break;
            }
            self.collect(&frames, &mut out);
        }
        if self.total_emitted > expected {
            let overflow = self.total_emitted - expected;
            let new_len = out.len().saturating_sub(overflow);
            out.truncate(new_len);
        }
        Ok(out)
    }

    /// Run one `process_into_buffer` call. `partial_len` lets the final chunk be
    /// shorter than `input_frames_next()`; rubato zero-pads the missing tail.
    fn process(&mut self, chunk: &[f32], partial_len: Option<usize>) -> Result<Vec<f32>, String> {
        let input_adapter = SequentialSlice::new(chunk, 1, chunk.len())
            .map_err(|e| format!("Failed to create input adapter: {:?}", e))?;
        let out_cap = self.resampler.output_frames_max().max(1);
        let mut output = vec![0f32; out_cap];
        let mut output_adapter = SequentialSlice::new_mut(&mut output, 1, out_cap)
            .map_err(|e| format!("Failed to create output adapter: {:?}", e))?;
        let indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            partial_len,
            active_channels_mask: None,
        };
        let (_nbr_in, nbr_out) = self
            .resampler
            .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
            .map_err(|e| format!("Streaming resample failed: {:?}", e))?;
        output.truncate(nbr_out);
        Ok(output)
    }

    /// Emit resampled frames, dropping the leading `output_delay()` latency once.
    fn collect(&mut self, frames: &[f32], out: &mut Vec<f32>) {
        let mut start = 0;
        if self.frames_to_skip > 0 {
            let skip = self.frames_to_skip.min(frames.len());
            self.frames_to_skip -= skip;
            start = skip;
        }
        out.extend_from_slice(&frames[start..]);
        self.total_emitted += frames.len() - start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_identity() {
        let input = vec![0.5f32; 16_000];
        let result = resample_to_16khz(&input, 16_000).unwrap();
        assert_eq!(input.len(), result.len());
    }

    #[test]
    fn test_resample_48khz_to_16khz() {
        let input = vec![0.5f32; 48_000];
        let result = resample_to_16khz(&input, 48_000).unwrap();
        let expected = (input.len() as f64 * 16_000.0 / 48_000.0).ceil() as usize;
        assert!(
            result.len() >= expected,
            "Expected at least {} output samples, got {}",
            expected,
            result.len()
        );
    }

    #[test]
    fn test_resample_24khz_to_16khz() {
        let input = vec![0.5f32; 24_000];
        let result = resample_to_16khz(&input, 24_000).unwrap();
        let expected = (input.len() as f64 * 16_000.0 / 24_000.0).ceil() as usize;
        assert!(
            result.len() >= expected,
            "Expected at least {} output samples, got {}",
            expected,
            result.len()
        );
    }

    #[test]
    fn test_resample_no_tail_loss_48khz() {
        // 5-second 48kHz sine wave — must not lose tail samples
        let sample_rate = 48_000u32;
        let num_samples = sample_rate as usize * 5;
        let input: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();

        let result = resample_to_16khz(&input, sample_rate).unwrap();
        let expected_min = (num_samples as f64 * 16_000.0 / sample_rate as f64).ceil() as usize;
        assert!(
            result.len() >= expected_min,
            "Expected at least {} output samples, got {} — tail loss detected",
            expected_min,
            result.len()
        );
    }

    #[test]
    fn test_resample_no_tail_loss_44100hz() {
        // 3-second 44.1kHz signal — must not lose tail samples
        let sample_rate = 44_100u32;
        let num_samples = sample_rate as usize * 3;
        let input: Vec<f32> = (0..num_samples)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();

        let result = resample_to_16khz(&input, sample_rate).unwrap();
        let expected_min = (num_samples as f64 * 16_000.0 / sample_rate as f64).ceil() as usize;
        assert!(
            result.len() >= expected_min,
            "Expected at least {} output samples, got {} — tail loss detected",
            expected_min,
            result.len()
        );
    }

    #[test]
    fn test_resample_short_input() {
        // 100 samples at 48kHz — must produce output
        let input: Vec<f32> = (0..100).map(|i| i as f32 * 0.01).collect();
        let result = resample_to_16khz(&input, 48_000).unwrap();
        assert!(!result.is_empty(), "Short input must produce some output");
        let expected_min = (100_f64 * 16_000.0 / 48_000.0).ceil() as usize;
        assert!(
            result.len() >= expected_min,
            "Expected at least {} output samples, got {}",
            expected_min,
            result.len()
        );
    }

    #[test]
    fn test_resample_tail_content_preserved() {
        // Last 10% has 9x amplitude — tail must survive resampling
        let sample_rate = 48_000u32;
        let num_samples = 48_000usize;
        let boundary = (num_samples as f64 * 0.9) as usize;

        let input: Vec<f32> = (0..num_samples)
            .map(|i| {
                let amp = if i < boundary { 0.1f32 } else { 0.9f32 };
                amp * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin()
            })
            .collect();

        let result = resample_to_16khz(&input, sample_rate).unwrap();

        let output_boundary = (result.len() as f64 * 0.9) as usize;
        let head = &result[..output_boundary];
        let tail = &result[output_boundary..];

        let head_rms = (head.iter().map(|x| x * x).sum::<f32>() / head.len() as f32).sqrt();
        let tail_rms = (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt();

        assert!(
            tail_rms > head_rms * 3.0,
            "Tail RMS ({:.4}) should be much higher than head RMS ({:.4}) — last 10% was lost",
            tail_rms,
            head_rms
        );
    }

    #[test]
    fn streaming_resampler_matches_batch_length() {
        // 1 s of 48 kHz sine -> ~16000 output samples, within the same rounding
        // as the batch path.
        let sr = 48_000u32;
        let n = sr as usize;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin() * 0.5)
            .collect();

        let batch = resample_to_16khz(&input, sr).unwrap();
        let mut stream = StreamingResampler::new(sr as usize).unwrap();
        let mut out = stream.push(&input).unwrap();
        out.extend(stream.finish().unwrap());

        let expected = (n as f64 * 16_000.0 / sr as f64).ceil() as usize;
        assert!(
            (out.len() as i64 - expected as i64).abs() <= 1,
            "streaming len {} should match expected {} within 1",
            out.len(),
            expected
        );
        assert!(
            (batch.len() as i64 - out.len() as i64).abs() <= 2,
            "streaming len {} should match batch len {} within 2",
            out.len(),
            batch.len()
        );
    }

    #[test]
    fn streaming_resampler_handles_tiny_chunks() {
        // Feed 48 kHz input 7 samples at a time — must still produce full output
        // and the leading latency must be trimmed (no all-silence result).
        let sr = 48_000u32;
        let n = sr as usize;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin() * 0.5 + 0.1)
            .collect();

        let mut stream = StreamingResampler::new(sr as usize).unwrap();
        let mut out = Vec::new();
        for chunk in input.chunks(7) {
            out.extend(stream.push(chunk).unwrap());
        }
        out.extend(stream.finish().unwrap());

        let expected = (n as f64 * 16_000.0 / sr as f64).ceil() as usize;
        assert!(
            (out.len() as i64 - expected as i64).abs() <= 1,
            "chunked streaming len {} should match expected {}",
            out.len(),
            expected
        );
        assert!(
            !out.iter().all(|&x| x == 0.0),
            "output must not be all silence (latency trim failed)"
        );
    }
}
