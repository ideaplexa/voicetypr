//! Pure, unit-testable decode-ahead buffer for Whisper live preview (plan 032).
//!
//! Sliding-window decode-ahead (ref: `itsmontoya/scribble` `BufferedSegmentTranscriber`):
//! a growing `samples` buffer plus a `head` index; `window()` is the un-decoded tail
//! `samples[head..]`. whisper.cpp has no KV/encoder-state reuse, so each decode re-runs
//! `whisper_full` on the whole window. This buffer decides WHEN to decode, which
//! segments to COMMIT (append-only) vs. hold as TENTATIVE, and how far to ADVANCE the
//! head so committed audio is never re-decoded.
//!
//! Deliberately pure: no whisper, no threads, no I/O, no tauri. The sink/threading
//! integration lives in the separate Claude slice. Audio is 16 kHz mono f32 — the sink
//! resamples device-rate frames down to this before `push`.

/// Sample rate assumed throughout (16 kHz mono f32).
pub(crate) const SAMPLE_RATE: usize = 16_000;
/// Centiseconds → samples: 1 cs = 1/100 s = `SAMPLE_RATE / 100` = 160 samples.
const SAMPLES_PER_CENTISECOND: usize = SAMPLE_RATE / 100;
/// Maximum backoff shift: `incr_step * 2^N`, capped at `2^4 = 16×`.
const MAX_BACKOFF_SHIFT: u32 = 4;

/// Abstract `whisper_full` output for one segment: its text plus the END timestamp in
/// centiseconds (1/100 s), measured from the START of the decoded window.
/// `end_cs * 160` = samples from the window's head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecodedSegment {
    pub text: String,
    pub end_cs: i64,
}

/// Emitted preview state. `committed` grows append-only (byte-prefix monotonic, so it
/// satisfies `StreamSessionGate::assert_committed_monotonic`); `tentative` is the
/// concatenation of not-yet-committed segment texts, replaced wholesale each decode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DecodeAheadPartial {
    pub committed: String,
    pub tentative: String,
}

/// Tunable decode-ahead knobs. Defaults assume 16 kHz mono f32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    /// Minimum un-decoded samples before a decode is worth running (~1s). Below this,
    /// `should_decode` short-circuits to false (unless `eos`).
    pub(crate) min_window_samples: usize,
    /// Re-decode once the window has grown this many samples since the last attempt.
    pub(crate) incr_step_samples: usize,
    /// Hard cap: force a decode (committing ALL segments) once the window reaches this.
    pub(crate) max_window_samples: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_window_samples: 16_000,  // 1s @ 16k
            incr_step_samples: 16_000,   // 1s
            max_window_samples: 480_000, // 30s
        }
    }
}

/// Sliding-window decode-ahead buffer (pure logic).
pub(crate) struct DecodeAheadBuffer {
    samples: Vec<f32>,
    head: usize,
    committed: String,
    /// Window-length threshold at/after which the next decode is allowed. Compared
    /// against `window().len()` (a length, not an absolute index).
    next_infer_at_len: usize,
    /// Consecutive decodes that committed nothing; drives exponential backoff.
    no_progress_runs: u32,
    cfg: Config,
}

impl DecodeAheadBuffer {
    pub(crate) fn new(cfg: Config) -> Self {
        Self {
            next_infer_at_len: cfg.min_window_samples,
            cfg,
            samples: Vec::new(),
            head: 0,
            committed: String::new(),
            no_progress_runs: 0,
        }
    }

    pub(crate) fn with_default_config() -> Self {
        Self::new(Config::default())
    }

    /// Append decode-ready samples (16 kHz mono f32).
    pub(crate) fn push(&mut self, samples: &[f32]) {
        self.samples.extend_from_slice(samples);
    }

    /// The un-decoded tail: `samples[head..]`. `head` is always `<= samples.len()`
    /// (see `ingest`'s clamp), so this never panics.
    pub(crate) fn window(&self) -> &[f32] {
        &self.samples[self.head..]
    }

    /// Decide whether to decode now.
    /// - Skip while `!eos && window < min_window` (not enough audio yet).
    /// - Force when `eos` or `window >= max_window`.
    /// - Otherwise decode once the window reaches `next_infer_at_len`.
    pub(crate) fn should_decode(&self, eos: bool) -> bool {
        let window_len = self.window().len();
        if !eos && window_len < self.cfg.min_window_samples {
            return false;
        }
        if eos || window_len >= self.cfg.max_window_samples {
            return true;
        }
        window_len >= self.next_infer_at_len
    }

    /// Absorb a fresh decode's segments and produce the preview partial.
    ///
    /// Emit rule:
    /// - `eos` or window at/over `max_window` → commit ALL segments.
    /// - else ≥2 segments → commit all-but-the-last (last stays tentative).
    /// - else (≤1 segment) → commit nothing (back off).
    ///
    /// Committing appends segment text VERBATIM to `committed` (byte-prefix monotonic)
    /// and advances `head` past the last committed segment's end so committed audio is
    /// never re-decoded.
    pub(crate) fn ingest(&mut self, segs: &[DecodedSegment], eos: bool) -> DecodeAheadPartial {
        let window_len = self.window().len();
        let force_all = eos || window_len >= self.cfg.max_window_samples;

        let commit_count = if force_all {
            segs.len()
        } else if segs.len() >= 2 {
            segs.len() - 1
        } else {
            0
        };

        if commit_count > 0 {
            for seg in &segs[..commit_count] {
                self.committed.push_str(&seg.text);
            }
            // Advance head past the last COMMITTED segment's end (window-relative).
            let advance = end_cs_to_samples(segs[commit_count - 1].end_cs);
            let max_advance = self.samples.len() - self.head; // never advance past the end
            self.head += advance.min(max_advance);
        }

        // tentative = text of the segments NOT committed (the trailing one(s)).
        let tentative: String = segs[commit_count..]
            .iter()
            .map(|s| s.text.as_str())
            .collect();

        // Backoff + next-infer schedule, relative to the post-advance window.
        let progress = commit_count > 0;
        if progress {
            self.no_progress_runs = 0;
        } else {
            self.no_progress_runs = self.no_progress_runs.saturating_add(1);
        }
        let step = if progress {
            self.cfg.incr_step_samples
        } else {
            let shift = self.no_progress_runs.min(MAX_BACKOFF_SHIFT);
            self.cfg.incr_step_samples.saturating_mul(1usize << shift)
        };
        self.next_infer_at_len = self.window().len().saturating_add(step);

        self.maybe_compact();

        DecodeAheadPartial {
            committed: self.committed.clone(),
            tentative,
        }
    }

    /// Reclaim consumed samples once ≥1s has been committed-and-skipped OR the head has
    /// passed the halfway mark. Bounds memory for long recordings.
    ///
    /// Drains `samples[0..head]` and resets `head` to 0. This does NOT change
    /// `window().len()` — the un-decoded tail keeps its length and contents — so the
    /// window-relative `next_infer_at_len` needs NO adjustment: it already encodes "how
    /// much the WINDOW must grow", and the window is unchanged by compaction.
    /// (Subtracting the drained count here would collapse the grow-gap and trigger
    /// premature re-decode thrash right after every compaction.)
    fn maybe_compact(&mut self) {
        if self.head >= SAMPLE_RATE || self.head > self.samples.len() / 2 {
            self.samples.drain(0..self.head);
            self.head = 0;
        }
    }

    pub(crate) fn committed(&self) -> &str {
        &self.committed
    }
}

/// Convert a whisper segment end timestamp (centiseconds from window start) into samples
/// (16 kHz). 1 cs = 160 samples; e.g. `end_cs = 100` → 1 s → 16 000 samples.
/// Non-positive timestamps advance nothing.
fn end_cs_to_samples(end_cs: i64) -> usize {
    if end_cs <= 0 {
        return 0;
    }
    (end_cs as u64).saturating_mul(SAMPLES_PER_CENTISECOND as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::stream::StreamSessionGate;

    /// Test helper: build a `DecodedSegment`.
    fn seg(text: &str, end_cs: i64) -> DecodedSegment {
        DecodedSegment {
            text: text.to_string(),
            end_cs,
        }
    }

    /// `n` distinguishable samples: `samples[i] = i as f32`.
    fn samples(n: usize) -> Vec<f32> {
        (0..n).map(|i| i as f32).collect()
    }

    // ---- push / window ----

    #[test]
    fn push_appends_samples_and_window_grows() {
        let mut b = DecodeAheadBuffer::with_default_config();
        assert_eq!(b.window().len(), 0);
        b.push(&samples(1_000));
        assert_eq!(b.window().len(), 1_000);
        b.push(&samples(500));
        assert_eq!(b.window().len(), 1_500);
    }

    #[test]
    fn window_is_the_undecoded_tail_from_head() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(10_000));
        // commit one segment ending at 0.5s -> head advances 8000 samples.
        b.ingest(&[seg("a", 50), seg("b", 80)], false);
        assert_eq!(b.window().len(), 2_000);
        // content is the original samples[8000..10000] (compaction is content-transparent).
        assert_eq!(b.window()[0], 8_000.0);
        assert_eq!(b.window()[1_999], 9_999.0);
    }

    // ---- should_decode gating ----

    #[test]
    fn should_decode_false_below_min_window_without_eos() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(15_999));
        assert!(!b.should_decode(false));
    }

    #[test]
    fn should_decode_true_at_min_window() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(16_000));
        assert!(b.should_decode(false));
    }

    #[test]
    fn should_decode_forced_true_at_eos_even_with_empty_window() {
        let b = DecodeAheadBuffer::with_default_config();
        assert_eq!(b.window().len(), 0);
        assert!(b.should_decode(true));
    }

    #[test]
    fn should_decode_forced_true_at_max_window_even_when_incr_gate_closed() {
        let cfg = Config {
            min_window_samples: 16_000,
            incr_step_samples: 16_000,
            max_window_samples: 40_000,
        };
        let mut b = DecodeAheadBuffer::new(cfg);
        b.push(&samples(20_000));
        // single-seg ingest -> no progress -> next_infer jumps to window + incr*2 = 52_000
        b.ingest(&[seg("a", 100)], false);
        assert!(b.next_infer_at_len > 40_000); // incr gate alone would say "not yet"
        b.push(&samples(20_000)); // window -> 40_000 == max
        assert_eq!(b.window().len(), 40_000);
        assert!(b.should_decode(false));
    }

    #[test]
    fn should_decode_respects_increment_step_after_ingest() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        assert!(b.should_decode(false));
        // commit one of two segs (end_cs 50 -> 8000 advance): window 12000, next = 28000
        b.ingest(&[seg("a", 50), seg("b", 100)], false);
        assert_eq!(b.window().len(), 12_000);
        assert!(!b.should_decode(false)); // below min_window again
        b.push(&samples(10_000)); // window -> 22_000 < 28_000
        assert!(!b.should_decode(false));
        b.push(&samples(6_000)); // window -> 28_000 == next
        assert!(b.should_decode(false));
    }

    // ---- emit rule ----

    #[test]
    fn ingest_two_segments_commits_all_but_last() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        let p = b.ingest(&[seg("hello ", 50), seg("world", 100)], false);
        assert_eq!(p.committed, "hello ");
        assert_eq!(p.tentative, "world");
        assert_eq!(b.committed(), "hello ");
    }

    #[test]
    fn ingest_single_segment_commits_nothing_and_is_tentative() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        let p = b.ingest(&[seg("maybe", 100)], false);
        assert_eq!(p.committed, "");
        assert_eq!(p.tentative, "maybe");
        assert_eq!(b.committed(), "");
        assert_eq!(b.head, 0); // no advance on no-progress
    }

    #[test]
    fn ingest_zero_segments_commits_nothing() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        let p = b.ingest(&[], false);
        assert_eq!(p.committed, "");
        assert_eq!(p.tentative, "");
    }

    #[test]
    fn ingest_at_eos_commits_everything_and_drains_window() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        let p = b.ingest(&[seg("a", 50), seg("b", 100), seg("c", 125)], true);
        assert_eq!(p.committed, "abc");
        assert_eq!(p.tentative, "");
        // last committed end_cs=125 -> 20_000 samples, clamped to window -> head == end
        assert_eq!(b.head, b.samples.len());
        assert_eq!(b.window().len(), 0);
    }

    #[test]
    fn ingest_at_max_window_commits_everything() {
        let cfg = Config {
            min_window_samples: 16_000,
            incr_step_samples: 16_000,
            max_window_samples: 40_000,
        };
        let mut b = DecodeAheadBuffer::new(cfg);
        b.push(&samples(40_000));
        let p = b.ingest(&[seg("x", 100), seg("y", 200)], false);
        assert_eq!(p.committed, "xy");
        assert_eq!(p.tentative, "");
    }

    #[test]
    fn head_advance_excludes_committed_audio_from_next_window() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(32_000));
        // commit first seg ending at 1s (16_000 samples). head reaches SAMPLE_RATE, so
        // maybe_compact drains the committed prefix and resets head to 0 — the window is
        // what matters here (committed audio must be gone from it).
        b.ingest(&[seg("done", 100), seg("live", 200)], false);
        assert_eq!(b.head, 0); // compaction fired (head hit SAMPLE_RATE)
        assert_eq!(b.window().len(), 16_000);
        assert_eq!(b.window()[0], 16_000.0); // committed audio gone from window
    }

    #[test]
    fn tentative_is_last_uncommitted_segment_only() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        // 3 segments, non-eos, non-max: commit first two, last is tentative
        let p = b.ingest(
            &[seg("one ", 30), seg("two ", 60), seg("three", 100)],
            false,
        );
        assert_eq!(p.committed, "one two ");
        assert_eq!(p.tentative, "three");
    }

    // ---- sample math ----

    #[test]
    fn end_cs_to_samples_one_second_is_16000() {
        assert_eq!(end_cs_to_samples(100), 16_000);
        assert_eq!(end_cs_to_samples(50), 8_000);
        assert_eq!(end_cs_to_samples(1), 160);
    }

    #[test]
    fn end_cs_non_positive_advances_zero_samples() {
        assert_eq!(end_cs_to_samples(0), 0);
        assert_eq!(end_cs_to_samples(-5), 0);
    }

    // ---- backoff ----

    #[test]
    fn backoff_grows_next_infer_threshold_on_no_progress() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000)); // window 20_000
        let w = b.window().len();
        b.ingest(&[seg("a", 100)], false); // no commit -> runs 1
        assert_eq!(b.no_progress_runs, 1);
        assert_eq!(b.next_infer_at_len, w + 16_000 * 2);
        b.ingest(&[seg("a", 100)], false); // runs 2
        assert_eq!(b.next_infer_at_len, w + 16_000 * 4);
        b.ingest(&[seg("a", 100)], false); // runs 3
        assert_eq!(b.next_infer_at_len, w + 16_000 * 8);
    }

    #[test]
    fn backoff_caps_at_sixteen_times_increment() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        let w = b.window().len();
        for _ in 0..10 {
            b.ingest(&[seg("a", 100)], false); // 2^shift caps at 2^4 = 16
        }
        assert_eq!(b.next_infer_at_len, w + 16_000 * 16);
    }

    #[test]
    fn backoff_resets_on_progress() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        b.ingest(&[seg("a", 100)], false); // runs 1
        b.ingest(&[seg("a", 100)], false); // runs 2
        assert_eq!(b.no_progress_runs, 2);
        b.ingest(&[seg("x", 100), seg("y", 200)], false); // progress -> reset
        assert_eq!(b.no_progress_runs, 0);
        assert_eq!(b.next_infer_at_len, b.window().len() + 16_000); // plain incr_step
    }

    // ---- committed monotonicity (the gate contract) ----

    #[test]
    fn committed_grows_append_only_across_ingests() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(32_000));
        let p1 = b.ingest(&[seg("alpha ", 50), seg("beta", 100)], false);
        b.push(&samples(16_000));
        let p2 = b.ingest(&[seg("gamma ", 150), seg("delta", 200)], false);
        b.push(&samples(16_000));
        let p3 = b.ingest(&[seg("epsilon ", 250), seg("zeta", 300)], false);

        assert!(p2.committed.starts_with(&p1.committed));
        assert!(p3.committed.starts_with(&p2.committed));
        assert_eq!(p1.committed, "alpha ");
        assert_eq!(p2.committed, "alpha gamma ");
        assert_eq!(p3.committed, "alpha gamma epsilon ");
    }

    #[test]
    fn committed_satisfies_stream_session_gate_monotonicity() {
        // Exercise the REAL contract used by the streaming gate across a multi-step
        // scenario (including head advance, compaction, and a final eos commit-all).
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(32_000));
        let mut prev = String::new();
        let steps: &[(&[DecodedSegment], bool)] = &[
            (&[seg("hello ", 50), seg("world", 100)], false),
            (&[seg("foo ", 150), seg("bar", 200)], false),
            (&[seg("baz ", 250), seg("qux", 300)], false),
        ];
        for (segs, eos) in steps {
            b.push(&samples(16_000));
            let p = b.ingest(segs, *eos);
            assert!(
                StreamSessionGate::assert_committed_monotonic(&prev, &p.committed),
                "monotonicity broke: prev={prev:?} next={:?}",
                p.committed
            );
            prev = p.committed;
        }
        let pfinal = b.ingest(&[seg("end", 350)], true);
        assert!(StreamSessionGate::assert_committed_monotonic(
            &prev,
            &pfinal.committed
        ));
    }

    // ---- compaction ----

    #[test]
    fn compaction_preserves_window_content_and_committed() {
        let mut b = DecodeAheadBuffer::with_default_config();
        let original: Vec<f32> = (0..32_000).map(|i| i as f32).collect();
        b.push(&original);
        // commit first seg ending at 1s -> advance 16_000 -> head == 16_000 (>= SAMPLE_RATE)
        let p = b.ingest(&[seg("committed ", 100), seg("tent", 200)], false);
        assert_eq!(p.committed, "committed ");
        assert_eq!(b.head, 0); // compacted
        assert_eq!(b.samples.len(), 16_000);
        let expected: Vec<f32> = (16_000..32_000).map(|i| i as f32).collect();
        assert_eq!(b.window(), expected.as_slice()); // content unchanged
        assert_eq!(b.committed(), "committed ");
    }

    #[test]
    fn compaction_triggers_when_head_passes_halfway() {
        let cfg = Config {
            min_window_samples: 16_000,
            incr_step_samples: 16_000,
            max_window_samples: 480_000,
        };
        let mut b = DecodeAheadBuffer::new(cfg);
        b.push(&samples(18_000));
        // commit first seg ending at 60cs -> 9_600 advance. head(9_600) < SAMPLE_RATE
        // but head(9_600) > len/2(9_000) -> compact via the halfway rule.
        b.ingest(&[seg("a", 60), seg("b", 100)], false);
        assert_eq!(b.head, 0);
        assert_eq!(b.samples.len(), 18_000 - 9_600);
    }

    #[test]
    fn compaction_does_not_fire_when_head_small() {
        let mut b = DecodeAheadBuffer::with_default_config();
        b.push(&samples(20_000));
        // commit first seg ending at 50cs -> 8_000 advance. head(8_000) < 16_000 and
        // head(8_000) <= len/2(10_000) -> no compaction.
        b.ingest(&[seg("a", 50), seg("b", 100)], false);
        assert_eq!(b.head, 8_000);
        assert_eq!(b.samples.len(), 20_000);
    }

    #[test]
    fn ingest_empty_at_eos_returns_empty_partial_without_panic() {
        let mut b = DecodeAheadBuffer::with_default_config();
        let p = b.ingest(&[], true);
        assert_eq!(p.committed, "");
        assert_eq!(p.tentative, "");
    }
}
