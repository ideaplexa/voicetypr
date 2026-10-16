//! One audio preparation policy for capture streams and complete-WAV fallback.
use crate::audio::resampler::StreamingResampler;

pub struct PcmConverter {
    channels: usize,
    pending: Vec<i16>,
    resampler: Option<StreamingResampler>,
}

impl PcmConverter {
    pub fn new(rate: u32, channels: u16) -> Result<Self, String> {
        if rate == 0 || channels == 0 || channels > 32 {
            return Err("Unsupported audio format".into());
        }
        Ok(Self {
            channels: channels as usize,
            pending: Vec::new(),
            resampler: if rate == 16_000 {
                None
            } else {
                Some(StreamingResampler::new(rate as usize)?)
            },
        })
    }
    pub fn push(&mut self, samples: &[i16]) -> Result<Vec<f32>, String> {
        self.pending.extend_from_slice(samples);
        let complete = self.pending.len() / self.channels * self.channels;
        let mono: Vec<f32> = self.pending[..complete]
            .chunks_exact(self.channels)
            .map(|frame| {
                frame
                    .iter()
                    .map(|sample| *sample as f32 / 32768.0)
                    .sum::<f32>()
                    / self.channels as f32
            })
            .collect();
        self.pending.drain(..complete);
        match self.resampler.as_mut() {
            Some(resampler) => resampler.push(&mono),
            None => Ok(mono),
        }
    }
    pub fn finish(self) -> Result<Vec<f32>, String> {
        if !self.pending.is_empty() {
            return Err("Incomplete audio frame".into());
        }
        match self.resampler {
            Some(resampler) => resampler.finish(),
            None => Ok(Vec::new()),
        }
    }
}

pub fn read_wav(
    path: &std::path::Path,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<Vec<f32>, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|_| "Unable to read transcription audio")?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 || spec.sample_format != hound::SampleFormat::Int {
        return Err("CrispASR requires PCM16 WAV input".into());
    }
    if reader.duration() as u64 > spec.sample_rate as u64 * 3600 {
        return Err("Audio exceeds the one-hour limit".into());
    }
    let mut converter = PcmConverter::new(spec.sample_rate, spec.channels)?;
    let mut audio = Vec::new();
    let mut buffer = Vec::with_capacity(4096 * spec.channels as usize);
    for sample in reader.samples::<i16>() {
        buffer.push(sample.map_err(|_| "Invalid transcription audio")?);
        if buffer.len() == buffer.capacity() {
            if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("Transcription cancelled".into());
            }
            audio.extend(converter.push(&buffer)?);
            buffer.clear();
        }
    }
    audio.extend(converter.push(&buffer)?);
    audio.extend(converter.finish()?);
    Ok(audio)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_stereo_chunks_preserve_frames() {
        let mut converter = PcmConverter::new(16_000, 2).unwrap();
        assert!(converter.push(&[32767]).unwrap().is_empty());
        assert_eq!(
            converter.push(&[-32767, 16384, 16384]).unwrap(),
            vec![0.0, 0.5]
        );
        assert!(converter.finish().unwrap().is_empty());
    }
    #[test]
    fn resampled_stream_includes_its_tail() {
        let mut converter = PcmConverter::new(48_000, 1).unwrap();
        let mut output = converter.push(&vec![800; 48_123]).unwrap();
        output.extend(converter.finish().unwrap());
        assert_eq!(output.len(), 16_041);
    }
}
