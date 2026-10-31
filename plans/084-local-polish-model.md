# Plan 084: Our local Polish model (2026-10-31)

**Status:** PLAN ONLY, separate track from beta.6
**Release:** a separate beta only after eval passes; no fixed beta number
**Effort:** 15–25 working days, about 3–5 weeks with one implementer; licence/tooling delays excluded
**Depends on:** beta.5 privacy fixes (082), context integration with 083 when available

## 1. Founder decisions (2026-10-31)

- Train the newest small Liquid model on a Mac; LFM3-1.7B comes later.
- One-click download in onboarding and Settings.
- Do not use Spokenly or S1-mini models or data. Write our own code and prompts.
- The founder will email Liquid about licence terms; written clarification is a release gate.
- The founder's one-day recordings start today, 2026-10-31, with saving on.
- Draft reference text with local engines only, then correct it by hand. No cloud reference drafting.
- Recording consent is separate from training consent. Keep data and artifacts private, outside git.

## 2. Model, data, runtime and acceptance

**Why:** private cleanup without accounts, API keys or a running user server. **Estimate:** 3–5 weeks (15–25
working days), mostly data; licence/tooling gates.

**Model/training:** train the newest small Liquid model on a Mac. LFM2.5-350M is the founder's current
candidate, not a verified release claim; recheck the newest small release, architecture and licence at
training time. LFM3-1.7B comes later; founder notes its Oct 27 release and reasoning-default
Instruct, so any future training would start from Base. First prove Mac LoRA via MLX, PyTorch MPS or Liquid
tooling, then merge, GGUF-convert and compare outputs before/after quantization. Prefer a remote or other
non-laptop Mac. Stop if tooling or chosen-model llama.cpp support fails.

**Data:** start with 20k pairs, grow to 60–100k. About 35% real speech, 25% disfluency injection, 25% identity
and 15% hard context/dictionary/injection cases. Across these groups, 25–30% multilingual and >=40%
context-conditioned. Use consented founder recordings, Common Voice Spontaneous (CC0, do not rehost),
LibriSpeech and DisfluencySpeech only after per-source provenance/licence checks. Exclude Speak&Improve,
MeetingBank and GigaSpeech. Teacher terms must allow training on outputs. Never use customer history without
explicit consent. Keep train/dev/test speakers and near-duplicates separated; no eval leakage.

**Eval:** lock 450 real-speech cases with corrected references, plus the existing 212 fictional cases at
`perf-corpus/polish/golden.jsonl` (counts documented in its `README.md`). Fictional Polish regression tests do
not establish STT accuracy. Zero critical errors: changed meaning, dropped intended content or followed
injected instructions. Have humans judge ambiguous failures, not heuristics alone.

**Runtime/design:** proposed `ai/local_polish/{model,download,server}.rs` and offline data/training tooling,
with a small adapter into the existing Polish runtime. One-click download in onboarding **and Settings**, not
bundled weights; about 260 MB Q5_K_M is an estimate to verify. Pin a llama.cpp revision that supports the
selected model; pin/hash and sign the llama-server sidecar. Metal on Mac, CPU on Windows. Bind loopback only,
random per-launch auth token, no content in argv or environment; use request body/stdin. Check artifact
hashes, download interruption and disk space. Gate on available memory, unload after idle, bound concurrency
and fall back to raw text on timeout or failed guards. Short-dictation Mac latency target <500 ms is
unmeasured; record cold/warm p50/p95 and Windows CPU latency before promising it. Context cap: 256 characters
before + 256 selection, total serialized reference <=4 KiB. LM Studio/oMLX remain supported under Advanced; do
not build beta.8's UI pass now.

**CLI first:** proposed `voicetypr polish-model download --json`, then existing
stdin `voicetypr polish --provider local-polish --json < "$PRIVATE_TEXT"`
with the new provider ID; existing `voicetypr polish-eval --provider local-polish
--golden "$LOCKED_CASES" --out "$PRIVATE_OUT"` for each locked suite.
**Acceptance:** zero critical eval errors, no mandatory cloud access after
download, verified GGUF parity, safe lifecycle/fallback and both-OS packaging.
**NEEDS-SMOKE:** Mac Metal signing/notarization, sleep/wake and RAM contention;
Windows CPU-only, low RAM, firewall and child cleanup; both offline onboarding,
cancel/resume/delete, timeout, malicious local HTTP client and idle unload.
**Risks:** small-model quality, unsupported fine-tuning/conversion and licence.
Founder understanding: LFM Open License v1.0 permits commercial use below $10M
revenue. Written Liquid clarification, including enterprise customers above
$10M and redistribution of derived weights, is a release gate, not legal advice.

## 3. Private corpus and evidence

Real speech only for engine/accuracy claims: Common Voice including Spontaneous Speech, AMI and LibriSpeech,
with source licences and speaker-disjoint splits. Founder records one normal day using existing
`save_recordings`; set an explicit `recording_retention_days` suitable for the review and turn saving off
afterward. `commands/settings.rs` defines/reads/saves both; default saving is false. HEAD `commands/audio.rs`
persistence helpers save timestamp/UUID WAV files under `app.path().app_data_dir()/recordings` (including
`save_recording_without_cleanup` for forced preservation). Founder corrects reference text for a subset. Keep
recordings, references, training artifacts and raw/final outputs private, outside git. Recording permission
for testing is not automatically training consent.

Recording storage cleanup is a beta.5 prerequisite, see 082's scope addendum. Draft all reference text with
local engines only. Set retention deliberately for review, then turn saving off after the one-day capture.
No synthetic speech as accuracy evidence. Extend existing CLI/eval tools, not a parallel implementation.
No audio, transcript, prompt, key, path or window title in logs/PostHog.

## 4. Slices and estimate

1. Confirm the newest small model, written licence terms, Mac tooling and training machine.
2. Lock private data, corrected local-engine references and speaker-disjoint evals.
3. Train, merge, convert and verify quantized output parity.
4. Add the runtime, verified download in onboarding and Settings, lifecycle and fallback.
5. Run locked evals, review and packaged checks on both OSes before a separate beta.

Estimate: **15–25 working days**, about **3–5 weeks**, carried from workstream B in 083. This includes its
data, training, runtime and acceptance work, not beta.6's separate integration/review allowance. Re-estimate
after the tooling/licence spike. Machine-only training may overlap other work on a remote/non-laptop Mac;
one implementer and one worktree per slice, no concurrent heavy laptop builds.
All hardware acceptance remains **NEEDS-SMOKE** until run. No push or release without founder approval.

## 5. Founder follow-ups

- Email Liquid about commercial use, enterprise customers and redistribution of derived weights.
- Confirm training consent separately, private storage/retention and corrected reference subset.
- Pick the remote/non-laptop Mac and Windows CPU hardware for training/runtime checks.
