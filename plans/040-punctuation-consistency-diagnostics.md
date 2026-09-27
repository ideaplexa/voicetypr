# 040 — Punctuation consistency and AI-formatting diagnostics

**Status: DONE — reviewer pass; focused checks and local UI smoke passed 2026-07-24**

## Problem

A Windows 2.0.4 report says Russian punctuation and grammar correction work intermittently. The submitted INFO log proves Whisper Large v3 completed, but does not explicitly record why AI Formatting was skipped. Raw Whisper punctuation and optional AI correction are therefore easy to confuse.

## Scope

1. Audit CPU and GPU Whisper decoding parameters that can affect ordinary punctuation, including language and token-suppression parity.
2. Change punctuation-related inference settings only if upstream and local evidence prove the current configuration is wrong.
3. Emit one privacy-safe INFO decision/outcome record for each desktop transcription: applied, disabled, mode-skipped, literal-locked, unchanged, or fallback. Do not log dictated text, prompts, keys, or target applications.
4. Clarify in the existing Formatting UI that speech models infer punctuation, while AI Formatting provides explicit grammar and punctuation correction.
5. Add focused behavioral tests for the diagnostic contract and guidance.

## Acceptance

- CPU and GPU-sidecar punctuation settings are intentionally aligned or their differences are documented by code/tests.
- A submitted INFO log identifies whether AI Formatting ran and why it did not run.
- Formatting settings tell users to select Clean Dictation when they want grammar and punctuation correction.
- Existing privacy constraints remain intact.
- Focused Rust and frontend tests, formatting, typecheck, lint, and a local UI smoke pass succeed.

## Validation

- `cargo test ai_formatting_outcome_explains_each_decision_path`
- `cargo clippy --lib -- -D warnings`
- `rustfmt --edition 2021 --check src-tauri/src/whisper/transcriber.rs src-tauri/src/writing.rs`
- `pnpm exec vitest run src/components/sections/__tests__/EnhancementsSection.test.tsx`
- `pnpm typecheck`
- `pnpm lint`
- Browser smoke rendered the real Formatting section with mocked Tauri IPC, confirmed the punctuation guidance, opened the Formatting guide, and visually verified the dialog.
- Independent reviewer pass after correcting the language-transform fallback classification.
