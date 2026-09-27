# Plan 047: Polish Provider UX + No-Input Fast Path

**Status:** COMPLETE — local-agent readiness stabilization and selector redesign verified 2026-08-10
**Priority:** P0
**Size:** M
**Depends on:** 044, 046

## Scope

1. Keep Polish provider/model setup directly below the Polish summary when requested; keep per-app rules in Advanced.
2. Split provider setup into **Cloud API** and **Local Agents** tabs with compact horizontal rows, model selection, and supported thinking/effort selection.
3. Run Pi through plain one-shot print mode (`-p`) with tools, sessions, extensions, skills, prompt templates, and context files disabled; surface actionable CLI/provider errors.
4. Restore application toasts to the top center.
5. Skip the speech engine only for capture evidence classified as exact digital-zero no-input; uncertain audio must continue through transcription.
6. Correct the shortcut empty-state layout, cloud-model hierarchy, and AI Polish metric accounting.

## Acceptance

- Polish **Change** reveals provider/model setup next to the Polish controls instead of opening the distant Advanced section.
- Provider setup has Cloud API and Local Agents tabs; each local row exposes install/auth state, refresh, model, and supported thinking/effort controls.
- Pi uses plain `-p` output and reports the real non-zero error instead of a generic JSON-response failure.
- Empty digital-zero recordings return to idle without launching Whisper or Parakeet; uncertain and speech-positive recordings remain unchanged.
- Toasts render at top center.
- Empty shortcut actions align to the right of their descriptions on desktop and stack cleanly on narrow widths.
- Cloud transcription cards present the model as primary and provider as secondary.
- AI Polish analytics distinguish attempts from successful application so attempted use is not reported as zero.
- Focused frontend and backend checks pass, and changed UI paths are visually exercised.
- Agent-CLI capability help is cached for the app process, and privacy-safe
  stage timings separate the capability probe from the model invocation.
- Local-agent rows remain mounted while probes and refreshes run; each row
  exposes its model and thinking fields immediately, with unsupported controls
  labeled as agent-managed.
- Model search is scoped to a large per-agent dialog, and explicit CLI-default
  selections persist alongside the selected local provider.

## Verification

- `pnpm typecheck` and `pnpm lint` passed.
- Focused UI suites passed: 49 tests across Polish, shortcuts, and models.
- Focused Rust suites passed: agent CLI (66 passed, 6 ignored), analytics (10), Parakeet sidecar (13), writing pipeline (20), and speech evidence (8).
- Native CUA exercise confirmed Change → Provider & model, Cloud API/Local Agents tabs, installed-agent status/model/thinking controls, right-aligned shortcut actions, and model-first cloud cards.
- Native startup confirmed the benign E5RT probe is reduced to one informational message and model loading continues.
- The exact-zero classifier regression proves only non-empty, finite, exact digital zero skips the engine; non-zero, uncertain, and speech-positive inputs continue.
- Review regression confirmed literal-preserved snippets are reported as skipped, not as AI attempts; successful unchanged AI output remains an attempt.
- Cloud STT model IDs were rechecked against current official provider
  documentation on 2026-08-14. The UI now exposes only the curated
  general-purpose choices with friendly labels: Soniox v5; GPT Transcribe and
  GPT-4o mini Transcribe; Whisper Large v3 Turbo and v3; Nova 3 and Nova 2;
  and Cohere Transcribe.
- Cloud model selection is persisted per provider and covered end to end by 44
  Rust cloud-STT tests plus focused component interaction tests. The focused
  frontend suite passed 14 tests; `pnpm typecheck`, `pnpm lint`, `pnpm build`,
  and `cargo clippy -- -D warnings` passed.
- A native development launch reached `APP_SETUP_COMPLETE`. The hidden window's
  accessibility surface was unresolved, so no new native visual claim is made;
  the changed selector and shortcut states were exercised through behavioral
  component tests instead.

- Four supplied Pi polish samples spent 4,575–7,166 ms in AI while local
  transcription took 70–160 ms. A matching direct
  `openai-codex/gpt-5.6-luna` invocation with thinking off took 4.46 s, while
  `pi --help` took 0.42 s. This identifies remote model latency as dominant;
  caching the repeated help probe removes the measured local 0.42 s overhead.
- Regression triage found 706 repeated Kilo Code probes in 9m14s. The
  `ai-ready` listener no longer reloads every provider, probe results are
  process-cached with explicit Refresh bypass, and focused tests prove ready
  controls remain mounted during refresh.
- Browser smoke at 1280px and 800px verified stable installed/missing rows,
  supported thinking controls, the per-agent model dialog, model search, and
  selection. The focused Polish suite passed 44/44 tests; targeted Rust
  reasoning-level and CLI-default persistence tests passed.