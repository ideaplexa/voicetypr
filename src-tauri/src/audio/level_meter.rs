use std::sync::mpsc::SyncSender;

/// Simple voice-optimized audio level meter
/// Maps normal speaking voice to 0.5-0.8 range for better UX
pub struct AudioLevelMeter {
    audio_level_tx: SyncSender<f64>,
    smoothed_level: f32,
    sample_count: usize,
    update_interval: usize,
}

impl AudioLevelMeter {
    pub fn new(
        sample_rate: u32,
        _channels: u32, // Not needed for simple RMS
        audio_level_tx: SyncSender<f64>,
    ) -> Result<Self, String> {
        Ok(Self {
            audio_level_tx,
            smoothed_level: 0.0,
            sample_count: 0,
            update_interval: (sample_rate as usize) / 10, // Update 10 times per second
        })
    }

    /// Process audio samples and send level updates
    pub fn process_samples(&mut self, samples: &[f32]) -> Result<(), String> {
        // Calculate RMS (Root Mean Square) - simple and effective for voice
        let sum: f32 = samples.iter().map(|x| x * x).sum();
        let rms = (sum / samples.len() as f32).sqrt();

        // Apply exponential smoothing to avoid jittery meter
        // 0.7 = smooth, 0.3 = responsive
        self.smoothed_level = self.smoothed_level * 0.7 + rms * 0.3;

        self.sample_count += samples.len();

        // Send level update at intervals
        if self.sample_count >= self.update_interval {
            self.sample_count = 0;

            // Map RMS to display level optimized for voice
            // Normal speaking voice RMS is typically 0.01-0.1
            // We map this to 0.2-0.9 for good visual feedback
            let display_level = map_voice_level(self.smoothed_level);

            let _ = self.audio_level_tx.try_send(display_level);
        }

        Ok(())
    }
}

/// Map RMS level to display level optimized for voice
fn map_voice_level(rms: f32) -> f64 {
    // These thresholds are tuned for typical speaking voice
    const SILENCE_THRESHOLD: f32 = 0.001; // Below this is silence
    const WHISPER_LEVEL: f32 = 0.005; // Quiet speech
    const NORMAL_SPEECH: f32 = 0.02; // Normal conversation
    const LOUD_SPEECH: f32 = 0.1; // Raised voice

    if rms < SILENCE_THRESHOLD {
        0.0
    } else if rms < WHISPER_LEVEL {
        // Map 0.001-0.005 to 0.0-0.3
        let normalized = (rms - SILENCE_THRESHOLD) / (WHISPER_LEVEL - SILENCE_THRESHOLD);
        (normalized * 0.3) as f64
    } else if rms < NORMAL_SPEECH {
        // Map 0.005-0.02 to 0.3-0.7 (most common range)
        let normalized = (rms - WHISPER_LEVEL) / (NORMAL_SPEECH - WHISPER_LEVEL);
        (0.3 + normalized * 0.4) as f64
    } else if rms < LOUD_SPEECH {
        // Map 0.02-0.1 to 0.7-0.95
        let normalized = (rms - NORMAL_SPEECH) / (LOUD_SPEECH - NORMAL_SPEECH);
        (0.7 + normalized * 0.25) as f64
    } else {
        // Cap at 0.95 to show headroom
        0.95
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_level_mapping() {
        // Silence
        assert_eq!(map_voice_level(0.0), 0.0);
        assert_eq!(map_voice_level(0.0005), 0.0);

        // Whisper range
        assert!((map_voice_level(0.003) - 0.15).abs() < 0.1);

        // Normal speech range
        assert!((map_voice_level(0.01) - 0.5).abs() < 0.1);
        assert!((map_voice_level(0.015) - 0.6).abs() < 0.1);

        // Loud speech
        assert!((map_voice_level(0.05) - 0.8).abs() < 0.1);

        // Very loud
        assert_eq!(map_voice_level(0.2), 0.95);
    }
}
