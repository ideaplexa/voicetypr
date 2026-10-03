use std::sync::mpsc::SyncSender;

/// Allocation-free, nonblocking peak meter. Keep peaks until successfully queued.
pub struct AudioLevelMeter {
    audio_level_tx: SyncSender<f64>,
    peak: f32,
    sample_count: usize,
    update_interval: usize,
}
impl AudioLevelMeter {
    pub fn new(
        sample_rate: u32,
        channels: u32,
        audio_level_tx: SyncSender<f64>,
    ) -> Result<Self, String> {
        Ok(Self {
            audio_level_tx,
            peak: 0.0,
            sample_count: 0,
            update_interval: ((sample_rate as usize * channels as usize) / 60).max(1),
        })
    }
    pub fn process_samples(&mut self, samples: &[f32]) -> Result<(), String> {
        for sample in samples {
            if sample.is_finite() {
                self.peak = self.peak.max(sample.abs());
            }
            self.sample_count += 1;
            if self.sample_count >= self.update_interval {
                self.sample_count = 0;
                if self
                    .audio_level_tx
                    .try_send(self.peak.clamp(0.0, 1.0) as f64)
                    .is_ok()
                {
                    self.peak = 0.0;
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn peak_not_average_and_backpressure_preserves_peak() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut meter = AudioLevelMeter::new(240, 1, tx).unwrap();
        meter.process_samples(&[0.0, -0.8, 0.0, 0.0]).unwrap();
        meter.process_samples(&[0.1, 0.9, 0.1, 0.1]).unwrap();
        assert!((rx.recv().unwrap() - 0.8).abs() < 1e-6);
        meter.process_samples(&[0.0; 4]).unwrap();
        assert!((rx.recv().unwrap() - 0.9).abs() < 1e-6);
        meter
            .process_samples(&[f32::NAN, f32::INFINITY, 0.0, 0.0])
            .unwrap();
        assert_eq!(rx.recv().unwrap(), 0.0);
    }
}
