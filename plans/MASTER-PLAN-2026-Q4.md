# VoiceTypr Master Plan — Q4 2026 (desktop first)

Status: DRAFT for founder review, 2026-09-27. Supersedes the ordering of older
plans; older plans are history, not constraints. Built from the 2026-09-27
session: branch recovery, Parakeet preview root cause, FluidAudio probe,
user-pain research, local-polish research, competitor + pricing research,
and the founder's product decisions.

## North star

> Speak naturally → get correct, faithful text → spend no time fixing it.

Every phase is judged on the same five numbers, measured on our own test sets
(Phase 1), never on vendor benchmarks:

| Metric | Meaning |
|---|---|
| Accepted without edit | Share of dictations the user keeps unchanged |
| Hard-word accuracy | Names, products, jargon, code identifiers spelled right |
| Meaning-change errors | Polish changed facts, numbers, hedges, or answered the dictation |
| Stop → text (p50/p95) | Time from releasing the hotkey to text at the cursor |
| Insertion success | Text actually landed in the target app, clipboard restored |

## Principles

1. **Desktop first** (macOS + Windows). Mobile starts after Phase 3.
2. **Local and cloud both.** Local = private, free to run, offline. Cloud = convenience, languages, managed speed/quality.
3. **Least AI necessary.** Skip Polish when text is already clean; never rewrite more than needed.
4. **Faithful by default.** Keep the speaker's words, tone, names, numbers and hedges. Never answer a dictated question.
5. **Evidence gate.** Nothing ships as "better" without before/after numbers on the test sets.
6. **Hardware-aware.** GPU users get GPU paths; CPU-only machines get tuned CPU defaults.

## Packaging and pricing (founder decisions)

- **Core — one-time.** $39 for 1 device today. With mobile, the entry tier becomes
  2 devices at $59–69. One year of updates; the app keeps working after that;
  discounted renewal for another year of updates. Includes local engines, live
  preview, vocabulary, BYOK cloud recognition and BYOK Polish, and local Polish
  once shipped.
- **Plus — subscription.** $9/month; yearly $79 (recommended) or $90. Includes
  Core. Managed cloud recognition + Polish paid from monthly **credits**;
  "speed / quality / best" modes burn credits at different rates; users can buy
  top-up credits (this is the pay-as-you-go path — no separate tier).
  Real-time Polish is in both tiers: Core runs it locally or with the user's
  own keys, Plus runs it managed. Sync
  across devices; mobile when it ships. Device policy: open (see decisions).
- Not VC-backed: default managed engines must be cheap per hour (Soniox
  realtime $0.12/h, Muse $0.18/h); premium engines (Gemini Live ~$0.54/h) cost
  more credits.

## Phase 0 — Foundation (now)

Goal: the branch is trustworthy and the obvious user-facing problems are gone.

**How we work.** Claude writes specs, reviews diffs, runs every real check and
commits. Through the Codex plugin: implementation = `task --write --model
gpt-6-sol --effort high`; adversarial review of every change = `task --model
gpt-6-astra --effort medium` with a review prompt; second opinion on hard
architecture calls = `gpt-6-astra --effort xhigh`. Nothing is pushed or
released without the founder's OK.

| # | Work | Proof of done |
|---|---|---|
| 0.1 | Promote 2.0.6-beta.11 (main) to stable 2.0.6 — Release run 36322042743 started 2026-09-27 | Release published; updater serves 2.0.6 |
| 0.1a | **Last words cut off at stop:** post-roll — keep capturing ~250 ms after key release (today `recording/hotkeys.rs` Released → `stop_recording` → `recorder.rs` Stop with no delay); pad ~250 ms silence before Whisper decode. Small 2.0.7 on main | Clips ending at key release keep their last word |
| 0.1b | **Observability:** privacy-safe PostHog numbers per dictation — speech energy still active in the last ~100 ms at stop (clipping flag), audio length vs word count, stop → text, engine + version, re-dictation/edit right after; dashboard per release | Regressions visible as numbers per engine/version |
| 0.2 | Commit the two verified Parakeet sidecar fixes + plans (local) | Sidecar build + token harness green |
| 0.3 | Merge `main` into this branch: keep main's good parts; where both solve the same thing keep the better one; ask the founder when unclear | Full gate green; smoke of all engines |
| 0.4 | ONE `AGENTS.md` written from the merged code — not reworded from old docs: architecture, pipeline stages, invariants, testing through the CLI, conventions (CLAUDE.md = a one-line `@AGENTS.md` import, since Claude Code reads only CLAUDE.md); archive stale plans | A new agent can work from AGENTS.md alone |
| 0.4a | **Clean core:** split the god files (on main: `commands/audio.rs` 8,145 lines, `cloud_stt/soniox.rs` 4,609, `lib.rs` 2,424, `commands/remote.rs` 2,196) into one dictation pipeline with explicit stages (capture → preprocess → recognize → vocabulary → Polish → deliver); Tauri commands and CLI become thin adapters. SOLID-style single-purpose modules, typed errors, explicit state. Behaviour-preserving slices behind tests; architecture gets a gpt-6-astra xhigh second opinion; plan 050 + the July architecture review are inputs, re-derived from merged code | Same tests green before/after each slice; no god files |
| 0.4b | **CLI parity:** every feature reachable from the CLI with `--json` — full pipeline incl. vocabulary + Polish, settings, vocabulary, providers, per-stage timings. Today `cli.rs` has 4 commands and `transcribe` skips vocabulary and Polish | Phase 1 scorecard harness runs entirely through the CLI |
| 0.5 | [Plan 070](070-parakeet-full-context-preview.md): Parakeet full-context live preview | Plan 070 acceptance (preview WER ≈ batch, no dropped audio) |
| 0.6 | ~~Hide Unified/Nemotron~~ REVERSED 2026-09-27: on real speech (jfk.wav + LibriSpeech + MLS de/es) Unified and Nemotron are good and stream clean live text from ~1 s; the "broken" verdict came from the synthetic TTS corpus. Make them the recommended live-preview engines; retire `perf-corpus/synthetic` for engine decisions | Real-speech suite green (see below) |
| 0.7 | Soniox: realtime WebSocket as the default transport for every dictation, REST upload only as fallback | Stop → text p50/p95 before/after on real clips |
| 0.8 | Quick wins: pre-roll audio buffer (first words not clipped), import vocabulary from Wispr Flow / Handy | Verified on Mac + Windows |
| 0.9 | Windows signing for website downloads (Store MSIX is already Microsoft-signed): apply to SignPath Foundation; fallback Azure Artifact Signing ($9.99/mo, US/CA/EU/UK only) or route Windows downloads to the Store | No SmartScreen warning on the direct installer |
| Ship | 2.1.0 beta | Founder smoke + OK |

## Phase 1 — Measurement (starts in parallel with Phase 0)

Goal: one honest scoreboard everything else is judged against.

1. **Recording set:** 50–100 real clips from the founder (names, jargon, code
   terms, self-corrections, lists, mixed languages), each with the correct text.
   Small capture tool: record → save audio → type/fix the right text.
2. **Polish golden set:** 300–500 raw → ideal pairs across categories: fillers,
   false starts, self-corrections, lists, emails, names, numbers, hedges,
   questions that must not be answered, already-clean text, code terms,
   Bangla/Hindi/Vietnamese/Chinese pass-through.
3. **Harness:** engines × Polish providers → the five north-star metrics, p50/p95,
   on M1, M4 and a Windows laptop (CPU and GPU).
4. **Opt-in data donation (later in the phase):** separate consent, off by
   default; audio + raw text + final text to our own storage (not an analytics
   tool); preview each clip before sending; delete any time; fixed retention;
   privacy policy updated and consent wording checked.

## Phase 2 — Final-text accuracy (the core bet)

Goal: VoiceTypr gets the user's words right, local or cloud.

1. **Vocabulary engine (all engines):**
   - Store: term, spoken aliases, scope (global / app / project), source
     (manual / learned / imported), usage.
   - Relevance selection per dictation (app, recency) — short lists, no global
     "Jeff → JEV".
   - Recognition-time biasing: Whisper prompt, Soniox context, Deepgram
     keyterms, Gemini custom vocabulary, Muse keywords, Apple contextual strings.
   - Local sounds-alike corrector for every engine, gated by recognizer
     confidence (Parakeet gives per-word confidence) — no model download.
   - Learn from corrections automatically: when the user retypes a word right
     after insertion, save the fix to their list and show "Saved to your list ·
     Undo" (undo removes it). Track which recognized word produced the fix so
     the same mistake is corrected for that user next time.
2. **Polish excellence with current providers (priority over training our own):**
   context assembly (app type, vocabulary, text before cursor with permission,
   user style), faithful-by-default prompt, verbatim toggle, diff + undo,
   never-answer guard, skip Polish when text is already clean, prompt caching.
3. **JEV experiment (cloud path only):** route skip / light / full Polish,
   dictation vs command, pick between vocabulary candidates, classify user
   corrections. Keep only where it beats the baseline on quality *and* latency.
4. **Engines:** FluidAudio upgrade (0.17.x) + Parakeet Ultra option, after 0.2.
5. **Hardware-aware defaults:** detect GPU; choose engine/model/threads
   accordingly; tuned CPU settings for machines without a GPU.

## Phase 3 — Speed and real time

1. **Real-time Polish:** polish each finished sentence while the user speaks,
   keep the last 1–2 sentences revisable, final pass at stop. Works with cloud
   providers and with the local Polish model (llama.cpp sidecar). Depends on
   stable live text (0.2, 0.4).
2. **Stop → text:** consider making the Parakeet stream final authoritative for
   short dictations (skip the post-stop batch decode) once proven equal.
3. **New cloud engines:** test Muse Voice Transcribe (cheapest, push-to-talk
   mode) and Gemini 3.5 Transcribe (smart cleanup + vocabulary, 85+ languages)
   on the recording set; winners become BYOK options and Plus managed engines.

4. **Back-to-back (pipelined) dictation** (founder idea, 2026-09-27): start the
   next recording while earlier dictations are still transcribing/polishing.
   Today `recording/hotkeys.rs` only starts from Idle/Error, so the hotkey is
   dead during Transcribing/Polish. Design: split capture state (idle/recording)
   from a processing-job queue; one ordered delivery worker (no clipboard/paste
   collisions); queue cap; pill shows "Recording · N processing"; Escape cancels
   only the current capture; text goes where the cursor is when each result is
   ready. The 0.4a clean-core design must model this from the start.

## Live preview for every engine (researched 2026-09-27)

Default ON for free/local engines and engines whose streaming costs about the
same; opt-in for metered engines where streaming costs more.

| Engine | Preview | How | Effort | Default |
|---|---|---|---|---|
| Parakeet Unified (EN) / Nemotron (multi) | Native streaming, clean on real speech | Built | Done | On |
| Parakeet TDT | After plan 070 | Full-context re-decode | M | On after 070 |
| Whisper local | Decode-ahead | Built, GPU-gated | Done | On with GPU |
| Soniox | Realtime WS (also faster final) | Default-transport switch | M | On |
| Deepgram | Realtime WS | Built | Done | On |
| Remote LAN | WS route on host (`/api/v1/transcribe/stream`), relay `TranscriptionStreamEvent`, reuse keychain-backed `X-Voicetypr-Key`, `supports_streaming` in `/api/v1/status` for old-host fallback | New | M | On |
| Muse (Meta) | `wss://api.meta.ai/v1/asr/realtime`, cumulative partials + final, PTT mode, auth in handshake frame | New engine | S | On |
| Gemini 3.5 Transcribe Live | Live API interim/final; 10-min session cap → reconnect; no guaranteed message ordering → client sequencing | New engine | M | Opt-in (~1.8x batch) |
| OpenAI | Realtime transcription sessions (24 kHz PCM, client commits turns) | New path | M | Opt-in (~3–4x batch, unverified) |
| Apple SpeechTranscriber (macOS 26) | Volatile/final results; Swift bridge; per-locale OS-managed assets | New engine | M | On where available |
| Groq, Cohere | No streaming API exists | Hybrid local draft preview later | — | Off |
| R2T2 | Stock llama.cpp doesn't stream it | Watch list | L | — |

Rules: identical normalization for preview and final; preview styled as a
draft (mismatch complaints: EnviousWispr #2575, Google live-caption stability
research). Order: Parakeet + Soniox (2.1 beta.2) → Remote LAN + Muse → Apple
on-device, Gemini Live, OpenAI.

## Phase 4 — Plus launch

1. Accounts, credit ledger, usage metering, fair-use and abuse limits.
2. Managed recognition + Polish proxy with speed / quality / best modes.
3. Billing: $9/month, $79–90/year, credit top-ups; Core license integration.
4. Sync of vocabulary, style and settings across devices.

## Phase 5 — Local intelligence (toward end of year)

1. **Local Polish v1:** llama.cpp server as a warm sidecar (in-process linking
   clashes with the Whisper library's copy of ggml). English: S1-mini vs
   SpeakoFlow Mini on the golden set; pre-load the prompt while the user speaks.
   Windows without GPU may need the cloud path.
2. **Own Polish model:** small Qwen3.5 fine-tune, multilingual (incl. Bangla,
   Hindi, Vietnamese, Chinese — no open cleanup model exists for them).
   Training data from consented user corrections (footnote: if we ever train
   on a cloud model's outputs, check that provider's terms first).
3. **Languages:** language packs (fine-tuned Whisper models such as PhoWhisper
   and IndicWhisper run on our existing Whisper engine), R2T2 / Qwen3-ASR via
   the same llama.cpp sidecar (issue #145; license check), Apple SpeechAnalyzer
   as a no-download engine on macOS 26+.

## Phase 6 — Mobile and ecosystem (after Phase 3)

- Expo app + keyboard: iOS keyboard extension via config plugin (iOS keyboards
  cannot use the mic → app-session hop, like Wispr), Android keyboard (mic
  allowed). Apple on-device dictation as the no-download default on iOS.
- Siri / Shortcuts / Action Button via App Intents; Android assistant later.
- Windows Copilot key: register VoiceTypr as a Copilot key provider in the
  Store package (cheap — can move earlier).
- Watch later.

## Marketing track (follows shipped features)

The site (voicetypr.com, read 2026-09-27) already has: "Pay once. Keep forever"
pricing, testimonials, changelog, guides, comparison pages (Wispr Flow,
Superwhisper, Aqua, Dragon, Otter), free tools, EN/ES. Marketing pushes happen
when features ship, not before.

- **Landing page refresh with a "What's coming" section** once Phase 0 lands:
  directions, not dates; update it as items ship.
- **Plus messaging risk:** the whole site is anti-subscription ("Most tools
  charge $10–15 a month, forever. Voicetypr is one payment."). Plus must read as
  optional managed cloud on top of a one-time app, never as a switch to SaaS.
- **Core "1 year of updates" vs the site's "Keep forever" / lifetime FAQ:**
  existing buyers keep what they were sold; new wording must be clear.
- **Site claims to fix when features change:** "99 languages with automatic
  detection" (the app defaults to English and doesn't offer auto-detect on
  local Whisper), SmartScreen FAQ (goes away with code signing).
- Launch moments: after Phase 0–2 (HN / Reddit / Product Hunt), Plus launch,
  local Polish model release, then open-source marketing at full speed.
- Positioning stays: local-first, private, one payment. No "Whisper" in copy.

## Decisions needed from the founder

Needed now:
1. **Start Phase 0?** Commit the two tested sidecar fixes; the implementer
   agent writes plan 070 code, Codex reviews, Claude runs the checks.
2. **Soniox speed fix — when do users get it?** As a small update from `main`
   soon, or together with this branch later?
3. **Bring `main`'s latest code into this branch** (the July hold) once plan 070 lands?

Needed later:
4. Bring back the optional Parakeet word-boosting download (~98 MB), removed in July?
5. S1-mini must be credited "S1-mini by Superwhisper" — OK on an About screen?
6. Plus: 3 devices or unlimited?
7. Opt-in recording sharing as described in Phase 1.4?
8. API keys to test Muse, Gemini 3.5 Transcribe and JEV; R2T2 license when we test it.

## Validation pass (2026-09-27)

Real speech, streamed in real time through the sidecar (FluidAudio 0.15.5), word error rate batch / live-stream final:

| Engine | English (5 clips: JFK + 4 LibriSpeech) | German (2 MLS) | Spanish (2 MLS) | First live text |
|---|---|---|---|---|
| TDT batch + decode-ahead preview | 0–8.8% / 0–6.7% | 7.4–18.8% / **28–59%** | 0–2.4% / 4.9–6.5% | 0.8–2.9 s |
| Unified English (native streaming) | 0–2.9% / 0–2.9% | — | — | 1.0–1.8 s |
| Nemotron multilingual (native streaming) | 0–6.7% / 0–6.7% | 11–28% / 15–34% | 0–4.9% / 0–4.9% | 1.0–2.1 s |

Reads: the synthetic corpus was misleading (Unified/Nemotron are fine); TDT decode-ahead still degrades non-English live text badly → plan 070 stays; MLS references are normalized book text, so German numbers are partly normalization.

Re-verified facts: post-roll 250 ms is defensible (Handy uses 450 ms offline hangover + 450 ms pre-roll; tune from the new post-roll metric); FluidAudio latest is 0.17.4, Parakeet Ultra beats v3 everywhere (int8 ~595 MB, same 25 languages); S1-mini naming clause confirmed; llama.cpp supports Qwen3-ASR (PR #19441) but R2T2's streaming mode may need its own runtime — test before planning on it; ggml duplicate-symbol issue unresolved → sidecar; Soniox realtime $0.12/h vs async $0.10/h, same context/vocabulary support, 5 h sessions; Gemini 3.5 Transcribe GA 2026-09-25; Muse zero-data-retention; Apple SpeechAnalyzer language list and vocabulary support must be verified on a macOS 26 build. Distribution: Store installs are Microsoft-signed (link the site's Windows button to the Store); Azure signing is US/Canada-only for individuals and no signing clears SmartScreen instantly; SignPath for a paid AGPL binary is untested — apply and ask; the EULA's anti-redistribution clauses likely conflict with AGPL (legal review).

## Evidence index (this session)

- Parakeet preview root cause + experiment: [plan 070](070-parakeet-full-context-preview.md), `docs/reports/2026-09-26-parakeet-recovery.md`.
- FluidAudio 0.17.4 probe: plan 070 appendix (`.tmp/fa-upgrade`).
- Soniox: `main` uses upload → job → poll every 1 s; realtime only in live-preview mode on this branch (`commands/audio.rs` `build_soniox_stream_sink_factory`).
- Parakeet has no vocabulary support (`provider_capabilities.rs`).
- Research: user pains (insertion, faithful cleanup, vocabulary, live preview), local Polish shortlist, competitor issues and pricing — summarized in the session; sources linked there.
