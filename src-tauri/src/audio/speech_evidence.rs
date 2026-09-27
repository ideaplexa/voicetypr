use super::normalizer::NormalizationMetrics;
use super::recorder::CaptureAudioMetrics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechEvidenceClass {
    SpeechPositive,
    HighConfidenceNoInput,
    HighConfidenceNoSpeech,
    Uncertain,
}

impl SpeechEvidenceClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SpeechPositive => "speech_positive",
            Self::HighConfidenceNoInput => "high_confidence_no_input",
            Self::HighConfidenceNoSpeech => "high_confidence_no_speech",
            Self::Uncertain => "uncertain",
        }
    }

    pub const fn would_skip_engine(self) -> bool {
        matches!(
            self,
            Self::HighConfidenceNoInput | Self::HighConfidenceNoSpeech
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechEvidenceOutcome {
    AbortedBeforeResult,
    CancelledBeforeEngine,
    PreparationFailure,
    SkippedNoInput,
    SkippedNoSpeech,
    RecordingTooShort,
    EngineSuccess,
    EngineFailure,
}

impl SpeechEvidenceOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::AbortedBeforeResult => "aborted_before_result",
            Self::CancelledBeforeEngine => "cancelled_before_engine",
            Self::PreparationFailure => "preparation_failure",
            Self::SkippedNoInput => "skipped_no_input",
            Self::SkippedNoSpeech => "skipped_no_speech",
            Self::RecordingTooShort => "recording_too_short",
            Self::EngineSuccess => "success",
            Self::EngineFailure => "failure",
        }
    }
}

/// Attempt-scoped evidence finalizer. Construct it before fallible preparation,
/// then move it into the transcription future. `Drop` emits exactly one summary,
/// including when Tokio aborts the future before its first poll or during an await.
pub struct SpeechEvidenceAttempt {
    engine: String,
    route: &'static str,
    capture: Option<CaptureAudioMetrics>,
    prepared: Option<NormalizationMetrics>,
    outcome: SpeechEvidenceOutcome,
    #[cfg(test)]
    drop_counter: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    #[cfg(test)]
    emitted_lines: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
}

impl SpeechEvidenceAttempt {
    pub fn new(engine: String, route: &'static str, capture: Option<CaptureAudioMetrics>) -> Self {
        Self {
            engine,
            route,
            capture,
            prepared: None,
            outcome: SpeechEvidenceOutcome::AbortedBeforeResult,
            #[cfg(test)]
            drop_counter: None,
            #[cfg(test)]
            emitted_lines: None,
        }
    }

    pub fn set_prepared(&mut self, prepared: Option<NormalizationMetrics>) {
        self.prepared = prepared;
    }

    pub fn set_outcome(&mut self, outcome: SpeechEvidenceOutcome) {
        self.outcome = outcome;
    }

    fn formatted_summary(&self) -> String {
        let class = classify_speech_evidence(self.capture, self.prepared);
        let capture = self.capture.map(|metrics| {
            serde_json::json!({
                "sample_count": metrics.sample_count,
                "duration_ms": metrics.duration_ms,
                "rms": finite_f64(metrics.rms),
                "peak": finite_f32(metrics.peak),
                "max_window_rms": finite_f32(metrics.max_window_rms),
                "ms_above_rms_floor": metrics.ms_above_rms_floor,
                "windows_above_rms_floor": metrics.windows_above_rms_floor,
                "sample_rate": metrics.sample_rate,
                "channels": metrics.channels,
                "sustained_speech": metrics.speech_detected,
            })
        });
        let prepared = self.prepared.map(|metrics| {
            serde_json::json!({
                "pre_gain_peak": finite_f32(metrics.pre_gain_peak),
                "speech_like_modulation": metrics.speech_like_modulation,
                "applied_gain": finite_f32(metrics.applied_gain),
                "input_duration_ms": metrics.input_duration_ms,
                "output_duration_ms": metrics.output_duration_ms,
                "trimmed_duration_ms": metrics.trimmed_duration_ms,
            })
        });
        let payload = serde_json::json!({
            "engine": self.engine,
            "route": self.route,
            "engine_outcome": self.outcome.as_str(),
            "class": class.as_str(),
            "would_skip_engine": class.would_skip_engine(),
            "capture": capture,
            "prepared": prepared,
        });
        format!("SPEECH_EVIDENCE {payload}")
    }
}

impl Drop for SpeechEvidenceAttempt {
    fn drop(&mut self) {
        let line = self.formatted_summary();
        log::info!("{}", line);
        #[cfg(test)]
        {
            if let Some(lines) = &self.emitted_lines {
                if let Ok(mut lines) = lines.lock() {
                    lines.push(line);
                }
            }
            if let Some(counter) = &self.drop_counter {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
}
pub fn classify_speech_evidence(
    capture: Option<CaptureAudioMetrics>,
    prepared: Option<NormalizationMetrics>,
) -> SpeechEvidenceClass {
    let capture_speech = capture
        .map(|metrics| metrics.speech_detected)
        .unwrap_or(false);
    let prepared_speech = prepared
        .map(|metrics| metrics.speech_like_modulation)
        .unwrap_or(false);
    if capture_speech || prepared_speech {
        return SpeechEvidenceClass::SpeechPositive;
    }

    let capture = match capture {
        Some(metrics) if metrics.sample_count > 0 => metrics,
        _ => return SpeechEvidenceClass::Uncertain,
    };

    if capture.rms == 0.0 && capture.peak == 0.0 {
        return SpeechEvidenceClass::HighConfidenceNoInput;
    }

    // Calibrated from SPEECH_EVIDENCE telemetry (dev logs 2026-08-20/21):
    // pure silence sits at rms ~0.0007 with every fixed window under the
    // floor, while real speech — even a 200ms quiet word — sustains many
    // above-floor milliseconds. A mic wake-up pop or click is only a few.
    // Aggregate floors alone would reject a quiet word buried in long silence,
    // and a window maximum alone lets a single pop through (observed live:
    // 746ms "silent" capture with a transient, engine hallucinated 2 chars).
    if !capture.speech_detected
        && capture.rms.is_finite()
        && capture.peak.is_finite()
        && capture.max_window_rms.is_finite()
        && capture.rms > 0.0
        && capture.rms < NO_SPEECH_RMS_FLOOR
        && capture.peak < NO_SPEECH_PEAK_CEILING
        && capture.ms_above_rms_floor <= NO_SPEECH_MAX_TRANSIENT_MS
    {
        return SpeechEvidenceClass::HighConfidenceNoSpeech;
    }

    SpeechEvidenceClass::Uncertain
}

/// Aggregate RMS below this with no sustained-speech latch is treated as
/// strong evidence no speech occurred (calibrated: silence observed at
/// rms <= 0.00071; quietest real speech at 0.0084 — 4x headroom above).
pub const NO_SPEECH_RMS_FLOOR: f64 = 0.002;

/// Peak ceiling for the no-speech class (calibrated: silence observed at
/// peak <= 0.018; spoken words peak far above 0.05).
pub const NO_SPEECH_PEAK_CEILING: f32 = 0.05;

/// Highest single-callback RMS ceiling that still counts as a silent window.
/// Room tone windows sit at ~0.0007; a quiet spoken word drives callback
/// windows well above 0.003 (calibrated, see plan 059).
pub const NO_SPEECH_WINDOW_RMS_FLOOR: f32 = 0.003;

/// How many measured milliseconds of above-floor audio a "silent" capture may
/// contain and still be classified no-speech. Fixed 5ms windows are independent
/// of device callback size but may lose up to one partial window at each speech
/// boundary, so the 20ms measured limit conservatively protects actual speech
/// longer than 30ms. Observed live 2026-08-22: a 746ms silent capture whose brief
/// transient let a hallucinated 2-char transcript through the window-max guard.
pub const NO_SPEECH_MAX_TRANSIENT_MS: u64 = 20;

fn finite_f64(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

fn finite_f32(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(
        sample_count: u64,
        rms: f64,
        peak: f32,
        speech_detected: bool,
    ) -> CaptureAudioMetrics {
        // Default: windows proportional to the aggregate (pure tone shape).
        capture_with_windows(sample_count, rms, peak, speech_detected, rms as f32, 0)
    }

    fn capture_with_windows(
        sample_count: u64,
        rms: f64,
        peak: f32,
        speech_detected: bool,
        max_window_rms: f32,
        ms_above_floor: u64,
    ) -> CaptureAudioMetrics {
        CaptureAudioMetrics {
            sample_count,
            duration_ms: 1000,
            rms,
            peak,
            max_window_rms,
            ms_above_rms_floor: ms_above_floor,
            windows_above_rms_floor: 0,
            sample_rate: 16_000,
            channels: 1,
            speech_detected,
            post_roll_ms: 0,
            post_roll_interrupted: false,
            post_roll_speech_detected: false,
        }
    }

    fn prepared(speech_like_modulation: bool) -> NormalizationMetrics {
        NormalizationMetrics {
            pre_gain_peak: 0.02,
            speech_like_modulation,
            applied_gain: 10.0,
            input_duration_ms: 1000,
            output_duration_ms: 1000,
            trimmed_duration_ms: 0,
        }
    }

    #[test]
    fn silence_below_calibrated_floor_is_high_confidence_no_speech() {
        // Observed real silence: rms 0.0007, peak 0.018, no latch.
        assert_eq!(
            classify_speech_evidence(Some(capture(171_008, 0.00071, 0.018, false)), None),
            SpeechEvidenceClass::HighConfidenceNoSpeech
        );
    }

    #[test]
    fn quiet_unlatched_capture_above_floor_stays_uncertain() {
        // Quiet but real energy above the rms floor must transcribe.
        assert_eq!(
            classify_speech_evidence(Some(capture(48_000, 0.0084, 0.12, false)), None),
            SpeechEvidenceClass::Uncertain
        );
    }

    #[test]
    fn low_rms_with_loud_peak_stays_uncertain() {
        // A transient click: quiet average, loud peak — never reject.
        assert_eq!(
            classify_speech_evidence(Some(capture(48_000, 0.0015, 0.4, false)), None),
            SpeechEvidenceClass::Uncertain
        );
    }

    #[test]
    fn rms_at_floor_boundary_is_uncertain() {
        assert_eq!(
            classify_speech_evidence(
                Some(capture(48_000, NO_SPEECH_RMS_FLOOR, 0.03, false)),
                None
            ),
            SpeechEvidenceClass::Uncertain
        );
        assert_eq!(
            classify_speech_evidence(
                Some(capture(48_000, 0.00199, NO_SPEECH_PEAK_CEILING, false)),
                None
            ),
            SpeechEvidenceClass::Uncertain
        );
    }

    #[test]
    fn quiet_short_word_inside_long_silence_is_not_rejected() {
        // Reviewer scenario: 200ms quiet word inside a 10s silent capture ->
        // aggregate rms ~0.0016, peak 0.04, unlatched, but the word is
        // ~200ms of above-floor audio — far above the transient budget.
        // Duration semantics: same verdict at ANY device buffer size.
        assert_eq!(
            classify_speech_evidence(
                Some(capture_with_windows(
                    480_000, 0.001_58, 0.04, false, 0.01, 200
                )),
                None
            ),
            SpeechEvidenceClass::Uncertain
        );
        // A short 50ms quiet syllable also transcribes.
        assert_eq!(
            classify_speech_evidence(
                Some(capture_with_windows(
                    480_000, 0.001_58, 0.04, false, 0.01, 50
                )),
                None
            ),
            SpeechEvidenceClass::Uncertain
        );
    }

    #[test]
    fn steady_silence_with_flat_windows_still_rejects() {
        // Pure room tone: every window at the same low level as the aggregate.
        assert_eq!(
            classify_speech_evidence(
                Some(capture_with_windows(
                    171_008, 0.000_71, 0.018, false, 0.000_8, 0
                )),
                None
            ),
            SpeechEvidenceClass::HighConfidenceNoSpeech
        );
    }

    #[test]
    fn transient_mic_pop_inside_silence_is_rejected() {
        // Observed live 2026-08-22: 746ms "silent" capture where a mic
        // wake-up transient let a hallucinated 2-char transcript through the
        // window-max guard and it got pasted. A transient is a few ms of
        // above-floor audio regardless of how callbacks stack it.
        assert_eq!(
            classify_speech_evidence(
                Some(capture_with_windows(
                    71_680, 0.000_795, 0.007_48, false, 0.004, 5
                )),
                None
            ),
            SpeechEvidenceClass::HighConfidenceNoSpeech
        );
        assert_eq!(
            classify_speech_evidence(
                Some(capture_with_windows(
                    71_680, 0.000_795, 0.007_48, false, 0.004, 20
                )),
                None
            ),
            SpeechEvidenceClass::HighConfidenceNoSpeech
        );
        // 21 measured milliseconds can represent >30ms after accounting for
        // fixed-window boundary uncertainty, so it must transcribe.
        assert_eq!(
            classify_speech_evidence(
                Some(capture_with_windows(
                    71_680, 0.000_795, 0.007_48, false, 0.004, 21
                )),
                None
            ),
            SpeechEvidenceClass::Uncertain
        );
    }

    #[test]
    fn latched_speech_never_classified_as_no_speech() {
        assert_eq!(
            classify_speech_evidence(Some(capture(48_000, 0.0001, 0.001, true)), None),
            SpeechEvidenceClass::SpeechPositive
        );
    }

    #[test]
    fn positive_capture_or_prepared_evidence_always_wins() {
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 0.0, 0.0, true)), None),
            SpeechEvidenceClass::SpeechPositive
        );
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 0.0, 0.0, false)), Some(prepared(true)),),
            SpeechEvidenceClass::SpeechPositive
        );
        assert_eq!(
            classify_speech_evidence(None, Some(prepared(true))),
            SpeechEvidenceClass::SpeechPositive
        );
    }

    #[test]
    fn only_finite_nonempty_digital_zero_is_high_confidence_no_input() {
        let class = classify_speech_evidence(Some(capture(16_000, 0.0, 0.0, false)), None);
        assert_eq!(class, SpeechEvidenceClass::HighConfidenceNoInput);
        assert!(class.would_skip_engine());

        for metrics in [
            capture(0, 0.0, 0.0, false),
            capture(16_000, 0.0, 1e-10, false),
            capture(16_000, f64::NAN, 0.0, false),
            capture(16_000, 0.0, f32::NAN, false),
            capture(16_000, f64::INFINITY, 0.0, false),
            capture(16_000, 0.0, f32::INFINITY, false),
            capture(16_000, -0.1, 0.0, false),
            capture(16_000, 0.0, -0.1, false),
        ] {
            assert_eq!(
                classify_speech_evidence(Some(metrics), None),
                SpeechEvidenceClass::Uncertain
            );
        }

        // Degenerate-but-nonzero silence with zero peak now classifies as
        // strong no-speech evidence (calibrated floor; see plan 059).
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 1e-12, 0.0, false)), None),
            SpeechEvidenceClass::HighConfidenceNoSpeech
        );
    }

    #[test]
    fn any_nonzero_or_missing_capture_evidence_remains_uncertain() {
        // Below-floor silence now rejects — the 059 contract change.
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 1e-12, 1e-10, false)), None,),
            SpeechEvidenceClass::HighConfidenceNoSpeech
        );
        assert_eq!(
            classify_speech_evidence(Some(capture(16_000, 0.000_01, 0.005, false)), None,),
            SpeechEvidenceClass::HighConfidenceNoSpeech
        );
        assert_eq!(
            classify_speech_evidence(None, Some(prepared(false))),
            SpeechEvidenceClass::Uncertain
        );
    }

    #[test]
    fn structured_json_escapes_labels_and_marks_missing_metrics_null() {
        let attempt = SpeechEvidenceAttempt {
            engine: "remote \"lab\"\nserver".to_string(),
            route: "remote",
            capture: Some(capture(16_000, 0.1, 0.3, true)),
            prepared: None,
            outcome: SpeechEvidenceOutcome::EngineSuccess,
            drop_counter: None,
            emitted_lines: None,
        };
        let line = attempt.formatted_summary();
        let payload = line.strip_prefix("SPEECH_EVIDENCE ").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(payload).unwrap();

        assert_eq!(parsed["engine"], "remote \"lab\"\nserver");
        assert_eq!(parsed["engine_outcome"], "success");
        assert_eq!(parsed["class"], "speech_positive");
        assert_eq!(parsed["would_skip_engine"], false);
        assert_eq!(parsed["capture"]["rms"], 0.1);
        assert!(parsed["prepared"].is_null());
        assert_eq!(line.lines().count(), 1);
    }

    #[tokio::test]
    async fn abort_before_first_poll_emits_exactly_once() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut attempt = SpeechEvidenceAttempt::new("whisper".to_string(), "local", None);
        attempt.drop_counter = Some(counter.clone());
        attempt.emitted_lines = Some(lines.clone());
        let future = async move {
            let _attempt = attempt;
            std::future::pending::<()>().await;
        };

        let handle = tokio::spawn(future);
        handle.abort();
        let _ = handle.await;

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
        let emitted = lines
            .lock()
            .ok()
            .map(|lines| lines.clone())
            .unwrap_or_default();
        assert_eq!(emitted.len(), 1);
        assert!(emitted[0].contains("\"engine_outcome\":\"aborted_before_result\""));
    }

    #[tokio::test]
    async fn abort_during_await_emits_exactly_once() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut attempt = SpeechEvidenceAttempt::new("openai".to_string(), "cloud", None);
        attempt.drop_counter = Some(counter.clone());
        attempt.emitted_lines = Some(lines.clone());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let _attempt = attempt;
            let _ = started_tx.send(());
            std::future::pending::<()>().await;
        });
        let _ = started_rx.await;

        handle.abort();
        let _ = handle.await;

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(lines.lock().ok().map(|lines| lines.len()), Some(1));
    }

    #[tokio::test]
    async fn task_panic_emits_aborted_outcome_exactly_once() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut attempt = SpeechEvidenceAttempt::new("remote".to_string(), "remote", None);
        attempt.drop_counter = Some(counter.clone());
        attempt.emitted_lines = Some(lines.clone());
        let handle = tokio::spawn(async move {
            let _attempt = attempt;
            panic!("simulated task panic");
        });

        let error = handle.await.unwrap_err();

        assert!(error.is_panic());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
        let emitted = lines
            .lock()
            .ok()
            .map(|lines| lines.clone())
            .unwrap_or_default();
        assert_eq!(emitted.len(), 1);
        assert!(emitted[0].contains("\"engine_outcome\":\"aborted_before_result\""));
    }

    #[test]
    fn every_explicit_terminal_outcome_emits_once() {
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let outcomes = [
            SpeechEvidenceOutcome::CancelledBeforeEngine,
            SpeechEvidenceOutcome::PreparationFailure,
            SpeechEvidenceOutcome::RecordingTooShort,
            SpeechEvidenceOutcome::EngineSuccess,
            SpeechEvidenceOutcome::EngineFailure,
        ];

        for outcome in outcomes {
            let mut attempt = SpeechEvidenceAttempt::new("whisper".to_string(), "local", None);
            attempt.emitted_lines = Some(lines.clone());
            attempt.set_outcome(outcome);
            drop(attempt);
        }

        let emitted = lines
            .lock()
            .ok()
            .map(|lines| lines.clone())
            .unwrap_or_default();
        assert_eq!(emitted.len(), outcomes.len());
        for (line, outcome) in emitted.iter().zip(outcomes) {
            assert!(line.contains(&format!("\"engine_outcome\":\"{}\"", outcome.as_str())));
        }
    }
}
