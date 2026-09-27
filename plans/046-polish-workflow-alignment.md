# Plan 046: Polish workflow alignment

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: Plans 016, 027, 040
- **Claimed**: Main, 2026-08-09
- **State**: DONE (code) — NEEDS-SMOKE 046-S1..S6

## Goal

Align the shipped Polish pipeline with the simplified product contract before the next Beta: users speak naturally, deterministic personalization teaches exact terms and saved text, the active-app rule selects one AI mode before Polish runs, and no hidden spoken-punctuation feature rewrites ordinary dictation.

## Product contract

- Polish owns automatic grammar, punctuation, spacing, false-start cleanup, and restrained paragraph breaks at clear topic or intent changes.
- App Rules select the effective reshaping preset before the single logical Polish stage. A generic app category remains only a lightweight hint when no explicit rule matches.
- Deterministic personalization remains available with or without Polish: Automatic Corrections, Words & Names, and Saved Text.
- Saved Text matches a spoken trigger as the whole utterance; literal entries bypass Polish.
- Spoken formatting commands are not a shipped feature in this release. Remove their hidden defaults and runtime stage rather than hiding them behind an unadvertised setting.
- Legacy global reshaping presets persist during desktop backend startup and normalize in every backend read path, without requiring the user to open the Polish screen.
- The persisted `PersonalDictation` value remains an upgrade-compatible internal
  name only; the App Rules UI labels that behavior `Polish Off`.

## Deliverables

1. Remove the hidden voice-command schema, defaults, runtime substitutions, and obsolete tests/callers.
2. Move legacy global reshape migration from the React screen mount into desktop backend startup, while keeping CLI reads normalization-only so they never race the desktop settings writer.
3. Rename Text Shortcuts to Saved Text and clarify trigger/body/literal behavior in the UI.
4. Add restrained paragraph guidance to default AI Polish without changing the stronger Writing, Notes, Message, or Code contracts.
5. Update focused backend/frontend behavior tests and add the affected manual runtime checks to `plans/SMOKE.md`.
6. Explain the migration in the 2.0.6 post-update dialog and remove
   `Personal Dictation` from customer-facing labels and errors.

## Verification

- Focused Rust tests cover default Polish paragraph guidance, startup preset
  normalization, and deterministic rule ordering; the source residue check
  confirms no active spoken-command schema or runtime stage remains.
- Focused frontend tests cover Saved Text guidance, the `Polish Off` label, and
  version-scoped migration announcement content.
- `pnpm typecheck`, `pnpm lint`, `pnpm test`, backend Rust tests, formatting, and Clippy pass.
- Manual Beta smoke remains required for natural punctuation/paragraphing, Saved Text, app-rule precedence, and upgraded-settings migration.

## Observed evidence

- `pnpm quality-gate` passed: 629 frontend tests and 1,355 backend tests passed,
  with 13 backend tests ignored; typecheck, lint, Clippy, and Rust formatting
  also passed.
- Native macOS development app launched successfully; the Polish page exposed
  Saved Text with the new guidance and no Voice Commands section.
- The 2.0.6 migration announcement was rendered at desktop and 375×667
  viewports; its hierarchy, wrapping, close control, and Dismiss action remained
  visible without overflow.
- The final adversarial convergence review returned PASS with no remaining
  evidence-backed P0–P2 findings.
- Provider-backed audio behavior and upgraded-profile migration remain assigned
  to the signed-Beta checks in `plans/SMOKE.md`.
- Recent-history app context now renders one app badge with the native macOS
  application icon; the internal category badge is not shown, and Ghostty is
  classified as a terminal instead of `Other`.
- The native icon conversion regression passed against Finder.app, the
  RecentRecordings render suite passed, and the isolated badge was visually
  exercised with the production stylesheet.

