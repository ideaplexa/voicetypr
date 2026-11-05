//! Overlap-free, 16 kHz mono audio segmentation for batch TDT inference.

pub const SAMPLE_RATE: usize = 16_000;
const MAX_SAMPLES: usize = 30 * SAMPLE_RATE;
const FALLBACK_SAMPLES: usize = 15 * SAMPLE_RATE;
const HOP: usize = SAMPLE_RATE / 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    pub start: usize,
    pub end: usize,
}

impl Chunk {
    #[cfg(test)]
    pub fn timestamp_offset_seconds(self) -> f32 {
        self.start as f32 / SAMPLE_RATE as f32
    }
}

pub fn split(samples: &[f32]) -> Vec<Chunk> {
    if samples.is_empty() {
        return Vec::new();
    }
    if samples.len() <= MAX_SAMPLES {
        return vec![Chunk {
            start: 0,
            end: samples.len(),
        }];
    }

    let quiet: Vec<bool> = samples
        .chunks(HOP)
        .map(|window| window.iter().all(|sample| sample.abs() < 0.003))
        .collect();
    let mut boundaries = vec![0];
    let mut quiet_start = None;
    for (i, is_quiet) in quiet
        .iter()
        .copied()
        .chain(std::iter::once(false))
        .enumerate()
    {
        if is_quiet && quiet_start.is_none() {
            quiet_start = Some(i);
        }
        if !is_quiet {
            if let Some(first) = quiet_start.take() {
                if i - first >= 8 {
                    let boundary = ((first + i) * HOP / 2).min(samples.len());
                    if boundary - boundaries[boundaries.len() - 1] >= 2 * SAMPLE_RATE
                        && samples.len() - boundary >= 2 * SAMPLE_RATE
                    {
                        boundaries.push(boundary);
                    }
                }
            }
        }
    }
    boundaries.push(samples.len());

    let mut chunks = Vec::new();
    for pair in boundaries.windows(2) {
        let mut start = pair[0];
        while pair[1] - start > MAX_SAMPLES {
            chunks.push(Chunk {
                start,
                end: start + FALLBACK_SAMPLES,
            });
            start += FALLBACK_SAMPLES;
        }
        if start < pair[1] {
            chunks.push(Chunk {
                start,
                end: pair[1],
            });
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers_exactly_once(chunks: &[Chunk], len: usize) {
        assert!(chunks.iter().all(|chunk| chunk.start < chunk.end));
        assert_eq!(chunks.first().map(|chunk| chunk.start), Some(0));
        assert_eq!(chunks.last().map(|chunk| chunk.end), Some(len));
        assert!(chunks.windows(2).all(|pair| pair[0].end == pair[1].start));
    }

    #[test]
    fn continuous_speech_uses_fifteen_second_fallback() {
        let audio = vec![0.1; 65 * SAMPLE_RATE];
        let chunks = split(&audio);
        assert_eq!(chunks[0].end, 15 * SAMPLE_RATE);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.end - chunk.start <= 30 * SAMPLE_RATE));
        covers_exactly_once(&chunks, audio.len());
    }

    #[test]
    fn sustained_quiet_gap_is_split_without_overlap() {
        let mut audio = vec![0.1; 40 * SAMPLE_RATE];
        audio[18 * SAMPLE_RATE..20 * SAMPLE_RATE].fill(0.0);
        let chunks = split(&audio);
        assert_eq!(chunks.len(), 2);
        assert!((18 * SAMPLE_RATE..20 * SAMPLE_RATE).contains(&chunks[0].end));
        covers_exactly_once(&chunks, audio.len());
    }

    #[test]
    fn boundary_words_have_unique_offsets() {
        let audio = vec![0.1; 31 * SAMPLE_RATE];
        let chunks = split(&audio);
        covers_exactly_once(&chunks, audio.len());
        let boundary = chunks[0].end;
        let owner = |sample: usize| {
            chunks
                .iter()
                .filter(|c| c.start <= sample && sample < c.end)
                .count()
        };
        assert_eq!(owner(boundary - 1), 1);
        assert_eq!(owner(boundary), 1);
        assert_eq!(
            chunks[1].timestamp_offset_seconds(),
            boundary as f32 / SAMPLE_RATE as f32
        );
    }

    #[test]
    fn empty_short_and_exact_thirty_seconds() {
        assert!(split(&[]).is_empty());
        assert_eq!(split(&[0.1]), vec![Chunk { start: 0, end: 1 }]);
        assert_eq!(
            split(&vec![0.1; 30 * SAMPLE_RATE]),
            vec![Chunk {
                start: 0,
                end: 30 * SAMPLE_RATE
            }]
        );
    }
}
