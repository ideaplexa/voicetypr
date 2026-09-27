# Plan 034 — Onboarding: close the untested-model skip hole

> Smallest slice of the Handy-parity onboarding work (gating only — no flow
> restructuring, no changes to the success/upgrade business screens; those await a
> product decision). Grounding verified against main @ af63ab1 (Explore 2026-07-02).

## Verified current behavior

- Onboarding = src/components/onboarding/OnboardingDesktop.tsx (single component,
  string-union Step state machine :74-82,:225; step order :283-306).
- Completion is already model-gated: `readiness` step hard-gates on a
  downloaded+selected model or online remote (`sourceReady` :811-812), and
  AppContainer.tsx:266-270 force-reopens onboarding whenever
  `modelAvailability.hasModels === false`. That guard stays untouched.
- THE HOLE: `first_transcription` step (:1331-1412) verifies the pipeline works
  (user dictates "The quick brown fox…", gated on `hasCurrentSampleTranscript`
  :815-816) — but a "Skip for now" ghost button (:1341-1342) jumps straight to
  `success`, so a user can finish onboarding having never proven a working
  transcription. No test covers this path.

## Change

1. Make the skip explicit and informed (inline two-step confirm, no new dialog):
   - First click on "Skip for now" → button swaps to a confirm state with the label
     "Skip without testing?" plus one short helper line under it:
     "You can test anytime from the main window." A second click within ~5s
     confirms and proceeds to `success`; the state reverts on timeout or on any
     other interaction (navigating back, recording started).
   - If a sample transcription already succeeded (`hasCurrentSampleTranscript`),
     the button is not rendered at all (today it renders regardless — verify; if it
     already hides, keep that).
   - Starting a recording while the confirm state is active cancels the confirm.
2. Tests (extend src/components/onboarding/OnboardingDesktop.test.tsx):
   - skip requires two clicks: one click does NOT navigate; second click does.
   - confirm state reverts after the timeout (use fake timers).
   - completing via confirmed skip persists `onboarding_completed: true` at the
     upgrade step (same assertion pattern as :222,:250).
   - a successful sample transcription hides/never shows the skip button.
3. No other steps, no copy changes elsewhere, no backend changes.

## Acceptance
- pnpm typecheck, pnpm lint, pnpm test pass (all 24 existing onboarding tests must
  keep passing unmodified in intent — adjust selectors only if strictly needed).
- No Rust changes.
- Do not commit.
