# Plan 059 — No-speech gate ("said nothing → nothing happens")

**Status:** CODE COMPLETE — beta.8 candidate; packaged macOS/Windows smoke pending
**Priority:** P0 (recurring user-reported pain)
**Effort:** S
**Depends on:** — (independent; shares 058's stop-path context)

## Problem (user-reported 2026-08-21)

Press hotkey, say nothing, press again → the app transcribes room tone into a
hallucinated word ("yeah", "okay so") and — worse — runs it through AI polish
and inserts it. Expected: "no speech detected" and nothing else.

## Current behavior (traced, file:line evidence)

- Stop path snapshots `CaptureAudioMetrics { sample_count, duration_ms, rms,
  peak, sample_rate, channels, speech_detected }` at recorder.rs:825-836,
  handed to the command at audio.rs:4783-4799.
- Full metrics are already shadow-logged by `SpeechEvidenceAttempt` Drop
  (audio/speech_evidence.rs:88-127) — the repo-rule instrumentation already
  exists.
- Gates today:
  - Exact-zero gate only (audio.rs:4973-4989) via `classify_speech_evidence`
    (speech_evidence.rs:142-167), whose `Uncertain` class deliberately passes
    through.
  - Local Whisper: normalize-with-metrics then **normalized duration < 0.5s**
    gate (audio.rs:5327-5351) → a ≥0.5s recording of pure room tone passes and
    Whisper hallucinates on it.
  - **Cloud/Remote STT: normalization + duration gate bypassed entirely**
    (audio.rs:5217-5248) → even short room tone can reach the cloud engine.
  - Post-result `is_non_speech_transcript` marker list (audio.rs:1125-1137,
    5684-5708) catches `""`/`[blank_audio]`/`[noise]`-style strings after the
    fact — Whisper's natural-word hallucinations ("yeah", "okay so") are not
    covered, and the engine call already happened.

## Fix design (honors AGENTS.md gotcha #11)

**Decision point: before engine dispatch, on every path (local/cloud/remote),
by extending `classify_speech_evidence` past exact-zero while keeping
`Uncertain` → pass-through.**

Reject only when evidence of absence is strong — never reject uncertain audio:

```
no_speech = speech_detected == false
            AND 0 < rms < NO_SPEECH_RMS_FLOOR
            AND peak < NO_SPEECH_PEAK_CEILING
```

**Calibration (dev-log corpus 2026-08-20/21, 76 attempts — 2026-08-21):**
72 speech_positive (all latched; lowest aggregate rms 0.0084), 2 exact-zero
(already gated), 2 silence captures — the reported-bug cases — at rms
0.00068–0.00071 / peak 0.0087–0.0183, no latch. Shipped floors:
`NO_SPEECH_RMS_FLOOR = 0.002` (2.8x above observed silence, 4.2x below the
quietest unlatched risk zone), `NO_SPEECH_PEAK_CEILING = 0.05` (2.7x above
observed silence peaks). Latched speech is unreachable by construction; a
loud transient fails the peak conjunction; quiet unlatched speech above the
floor stays Uncertain and transcribes. Enforcement shipped default-on.

- Thresholds calibrated from the **existing** SpeechEvidence telemetry (query
  logged attempts: distribution of rms/peak for known-good short utterances vs.
  known-hallucination outcomes). No new instrumentation pass needed if the
  logged corpus suffices; else ship shadow-only first and calibrate from one
  beta's telemetry (enforcement flag default off → on).
- Duration is **context, not a gate**: short+loud speech must still transcribe.
- On reject: emit "No speech detected" outcome → pill toast, **no engine call,
  no polish call, no cursor insertion**, no history row (or an explicit
  `no_speech` history entry per existing outcome taxonomy — match how cancel is
  stored).
- Expand `is_non_speech_transcript` as the second line of defense (cheap,
  post-engine): include multi-word junk patterns; route its hits to the same
  "no speech detected" outcome instead of inserting.

## Non-goals

- No second full-buffer scan (reuse the metrics snapshot already taken).
- No engine-config changes (Whisper `no_speech_prob` tuning may be a follow-up
  if telemetry shows the pre-gate misses; note `no_speech_thold(0.6)` is
  already set at whisper/transcriber.rs:557-596).

## Acceptance

1. Gates green + focused unit tests for the gate predicate (boundary rms/peak,
   short-loud-speech passes, long-silence rejects, metrics-missing → pass-
   through, never reject on uncertain).
2. Telemetry: gate decision + evidence fields on every stop (extend
   SpeechEvidenceAttempt).
3. Smoke (SMOKE.md when claimed): hotkey with silence → pill says "No speech
   detected", nothing inserted, no polish request in logs; quiet whisper still
   transcribes; cloud STT path shows same behavior as local.
4. Beta gating: enforcement lands flag-default-on only after threshold

## Phase 2 — engine/VAD-layer noise discrimination (filed 2026-08-23 after external validation)

Live noise testing showed the energy gate's ceiling: loud or sustained
non-speech (breath/hum/rumble) is energy-indistinguishable from quiet real
speech, and parakeet hallucinates short transcripts from it. Independent
design review confirmed no existing in-repo signal can separate these
(envelope, modulation, latch all exhausted). External research found the
industry answer, cheapest first:

1. **Sidecar confidence (S)**: FluidAudio `AsrManager.transcribe` already
   returns an aggregate `confidence`; the Swift sidecar drops it (uses only
   text + duration — sidecar/parakeet-swift/Sources/main.swift:108-119,
   446-523). Serialize it in `TranscriptionResponse` and post-filter
   low-confidence short transcripts. FluidAudio docs:
   https://github.com/FluidInference/FluidAudio/blob/main/Documentation/API.md
2. **FluidAudio VAD pre-gate (M)**: the same library ships `VadManager`
   (Silero CoreML, ANE-optimized, 256ms chunks, threshold .85 default;
   noisy environments .3-.6). Wire into the sidecar before transcription.
   Industry precedent: faster-whisper defaults vad_filter=True; whisper.cpp
   mainline Silero VAD; Superwhisper "Remove Silence"; VoiceInk VAD toggle.
3. **Whisper post-filter (S, whisper path only)**: whisper-rs 0.16 exposes
   `WhisperSegment::no_speech_probability()`; docs mark `set_no_speech_thold`
   as NOT implemented (our call at transcriber.rs:557-596 is likely a no-op).
   Correct approach: post-hoc drop segments where no_speech_prob > .6 and
   avg token plog < -1 (whisper.cpp semantics).
4. Rust-side Silero (silero-vad-rust / voice_activity_detector crates) only
   if the recorder layer itself needs speech-probability — heavier dep (ort).

Shadow-first per repo rule: log confidence/VAD decisions alongside
SPEECH_EVIDENCE for one beta before enforcement.
## STOP conditions

- If telemetry corpus cannot separate speech/no-speech cleanly (false-negative
  risk on soft speech), keep shadow mode and file findings — do not guess a
  threshold.
