# Plan 048: Expanded Local Agent CLIs

**Status:** DONE — implemented and verified 2026-08-10
**Priority:** P1
**Size:** L
**Depends on:** 044, 047

## Scope

Add best-effort Polish adapters for Codex, Droid, Grok, OpenCode, and Cline. Preserve the existing cold-spawn timeout, bounded output, temporary working directory, process-group cleanup, raw-transcript fallback, and sanitized user-facing failures. Apply each CLI's strongest available controls for tools, project context, plugins, memory, and session persistence without claiming identical guarantees across third-party CLIs.

## Acceptance

- All five providers appear under Local Agents and can be probed independently.
- Each adapter uses the installed CLI's documented non-interactive mode and provider-specific output parser.
- Model selection uses CLI default unless a compatible explicit model is selected.
- Reasoning controls appear only where the adapter has a verified per-run setting.
- Missing, incompatible, unauthenticated, timed-out, and malformed CLI runs preserve the raw transcript and surface actionable errors.
- Focused Rust/frontend checks pass; installed adapters receive exact-command smoke coverage.

## Verification

- `pnpm typecheck`, `pnpm lint`, and the focused Polish Vitest suite pass: 44 tests.
- Rust adapter coverage passes: `cargo test agent_cli` (74 passed, 8 ignored), `cargo test commands::ai::tests` (20 passed), and `cargo test ai::catalog::tests` (7 passed).
- `cargo clippy --lib -- -D warnings` and Rust formatting checks pass.
- Exact real adapter round trips pass for Codex, Droid, Grok, OpenCode, and Cline.
- Browser smoke opened Polish → Local Agents and observed exactly Claude Code, pi, oh-my-pi, Codex, Droid, Grok, OpenCode, and Cline; Amp, Kilo Code, and Hermes were absent.
- Focused review findings were independently checked against installed CLI help and real invocations. Cline uses disposable `--data-dir` state and a valid retry count.
- Amp and Kilo Code were removed from the catalog, runtime, discovery, and UI by product decision on 2026-08-10. Hermes was removed by product decision on 2026-08-12.
