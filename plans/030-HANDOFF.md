# HANDOFF — AI Polish Overhaul (Plan 030)

> For any agent taking over this work. Read this fully before touching anything.
> Written 2026-07-10. Branch state at handoff: `feat/ai-polish-personalization` @ `b82f869a`, 35 commits ahead of `main`, all pushed, all gates green.

## 1. What this is

VoiceTypr (Tauri v2 desktop app: Rust backend `src-tauri/`, React/TS frontend `src/`) does offline voice transcription. "Polish" is the optional AI post-processing step that cleans the raw transcript before it's typed into the active app.

**Mission:** make Polish as good as Wispr Flow, with a two-persona split as the architectural spine:
- **Regular users**: amazing zero-config defaults, ONE switch. They never see complexity.
- **Advanced users**: deep configurability, tucked under Advanced.

Every design decision on this branch was made through that lens. Keep it.

## 2. Where the work lives

- **Worktree:** `/Volumes/1tb-drive/developer/oss/worktrees/voicetypr-ai-polish-personalization`
- **Branch:** `feat/ai-polish-personalization` (pushed to origin)
- **Plan docs:** `plans/030-ai-polish-personalization.md` (master), `plans/030-phase4b-app-context-spec.md`, `plans/030-phase4c-agent-cli-spec.md` (has EMPIRICAL FINDINGS — read before touching agent-CLI code)

## 3. Workflow rules (founder-mandated, do not deviate)

1. **Research before implementing.** Founder rule: "do good codebase research before you fix/implement anything to avoid regressions." If unsure → web research first, then ask the founder. It's 2026 — search accordingly.
2. **Tauri-first, not Rust-first.** Check for an existing Tauri plugin/solution before hand-rolling Rust.
3. **Role split for delegated work:** GLM 5.2 IMPLEMENTS, Codex REVIEWS, the orchestrating agent gates + reviews (dual review). GLM launch template:
   ```
   omp -p --auto-approve --no-title --no-skills --no-rules --no-extensions \
       --thinking xhigh --tools read,edit,grep,bash --max-time <N> \
       --cwd <worktree> --model zai/glm-5.2 "$(cat taskfile)"
   ```
   Minimal tools only, xhigh thinking. GLM sometimes hits `--max-time` after actually finishing — verify with `cargo check` + gates before assuming failure.
4. **Marketing rule:** NEVER say "Whisper" in user-facing copy (say local/offline AI). Founder approves ALL user-facing copy before ship.
5. **Don't over-checkpoint.** Build + gate autonomously; surface only real decisions/blockers/completions. Founder: "keep moving."
6. **Never push without explicit founder instruction** (this branch has standing push permission; a new branch does not).

## 4. Gates (all must be green before commit)

```bash
pnpm typecheck && pnpm lint && pnpm test        # frontend (589 tests at handoff)
cd src-tauri && cargo test && cargo clippy      # backend (1247 tests at handoff)
```
Known env quirks:
- macOS 26 (Tahoe): local whisper Metal encode fails in the harness — CI and shipped builds are fine; don't chase it.
- Transient cargo "failed to create dependency graph" → re-run with `CARGO_INCREMENTAL=0`.

## 5. What was built (35 commits, in narrative order)

### Phase 1 — polish quality core
- Pinned sampling params + bounded max output tokens for every polish call (`src-tauri/src/ai/`).
- Hardened polish prompt; translation is explicit opt-in only (`ai/prompts.rs`).
- **Output validation + repair before insertion** — garbage model output (echoes, wrappers, "Here's your text:", empty) is repaired or the RAW TRANSCRIPT is used. Raw transcript is the sacred fallback on every failure path. Never weaken this.
- Per-stage latency instrumentation.

### Phase 2 — state collapse (refactors, no behavior change)
- Split `writing.rs` god-file into `src-tauri/src/writing/` modules.
- **Deleted `WritingMode` entirely** — presets are the single source of truth, one resolver.
- Centralized recording-config cache invalidation.
- **Shared Rust/TS golden fixture** `tests/fixtures/preset-parity.json` — both sides assert preset definitions byte-for-byte. If you change a preset, update the fixture and BOTH test suites will tell you.

### Phase 3 — two-persona UX
- Three formatting screens merged into ONE Polish screen (`src/components/sections/EnhancementsSection.tsx` — the big file).
- Guided BYOK provider setup (not a paywall look); connected status.
- Onboarding simplified; recording pill says "Polishing…".
- Tray shows + toggles Polish; per-mode formatting shortcuts retired (only Toggle Polish remains).
- Lockstep fixes: tray/shortcut can never enable an inert Polish (preset normalizes to Clean if needed).

### Phase 4A — OpenRouter
- First-class provider via the `openai_compatible` runtime. Curated model catalog.
- Catalog is data-driven: `catalog.generated.json` + `overlay.json` + `generate.py`; `CatalogProvider.runtime` ∈ {`genai_adapter`, `openai_compatible`, `agent_cli`}; dispatch in `ai/executor.rs::execute_once`.

### Phase 4B — app-context awareness
- `src-tauri/src/writing/app_category.rs`: frontmost app name → whole-word match against curated lists → 9 categories (Chat/Email/Docs/Code/Terminal/Social/Notes/Browser/Other) → one behavioral sentence injected into the prompt via `category_prompt_hint`.
- Whole-word matching is load-bearing ("1Password" ≠ Docs, "Barcode Scanner" ≠ Code) — tested.
- **Unknown/Browser → NO hint** (safe neutral). Local-only; app identity never leaves the machine.

### Phase 4C — agent-CLI providers (the headline feature)
Claude Code, pi, oh-my-pi (omp) as NO-API-KEY polish providers: spawn the user's already-installed, already-logged-in CLI headless one-shot; their subscription pays; we configure nothing.

Key file: `src-tauri/src/ai/agent_cli.rs`. Architecture:
- `AgentCliSpec` owns each provider's input/output/auth modes, mandatory isolation flags, default-model policy, and fixed low/off reasoning policy.
- `run_isolated_command` and the model-list helpers use a unique `TempDir`, no shell, dedicated Unix process groups / Windows Job Objects, concurrent bounded stdout/stderr drains, `kill_on_drop(true)`, and one deadline covering stdin, group exit, drain, group kill, and reap. A drop guard also group-kills and schedules reaping when an outer executor cancellation drops the command future. Polish has a 9s budget; model listing has a 3s budget.
- **Login-shell PATH resolver** (cached `OnceLock`, `$SHELL -ilc` env dump with timeout+fallback) lets a Finder-launched macOS app find user CLIs. Every PATH candidate is searched; unsafe script launchers are skipped so a later native executable can win. Windows accepts `.exe` only.
- Dictated text is stdin for Claude/pi or one discrete positional argument for omp. Model ids and prompts are separate `Command::arg` values; no dictated text enters a shell command string.
- Nonzero exits, write/read failures, malformed JSON, timeouts, and capped output are errors. Bounded, control-free CLI guidance may reach the toast, but apparent assistant text from a failed process can never be accepted. The raw transcript remains the fallback.

**Current invocation contracts:**
- Claude Code: `claude --safe-mode -p --tools "" --strict-mcp-config --no-chrome --model <selected-or-haiku> [--effort low] --system-prompt <PROMPT> --output-format json`. Older compatible versions fall back to the empirically proven `--setting-sources ""` isolation when `--safe-mode` is absent.
- pi: `pi -p --no-tools --no-session --no-extensions --no-skills --no-prompt-templates --no-context-files --thinking off [--model <provider/id>] --mode json --system-prompt <PROMPT>`, with dictation on stdin.
- omp: `omp -p --no-tools --no-lsp --no-session --no-extensions --no-skills --no-rules --no-title --thinking off [--model <selector>] --mode json --system-prompt <PROMPT> <dictation>`.

**EMPIRICAL FINDINGS (hard-won; re-test changes on a real logged-in machine):**
- `--bare` broke Claude subscription auth. On 2026-08-04, `claude --safe-mode auth status` preserved authentication, but a real polish call returned the account's current `403 subscription disabled` response; successful subscription-backed polish still needs founder smoke.
- pi's RPC `get_available_models` and `omp models --json --no-extensions` returned nonempty model catalogs without a completion request. Their final model selectors are passed through unchanged.
- pi/omp `--mode json` is JSONL; polished text is the last assistant message. omp requires dictation as a positional argument.
- Warm sessions remain out of scope: cold spawn is model-call-dominated and avoids lifecycle complexity.
- Full-trust MSIX can spawn external CLIs, but physical Windows smoke remains merge-blocking because CI is compile-only.

### Frontend for 4C
- `AgentCliProbe.state` distinguishes missing, unsafe launcher, incompatible CLI, not authenticated, and ready without exposing executable paths.
- Guided setup keeps the recommended/default model. Advanced reuses the existing searchable model picker: Claude gets curated Haiku/Sonnet/Opus; pi/omp get an explicit CLI-default choice plus models grouped by source provider.
- `list_provider_models` routes agent CLIs through curated or no-completion discovery. Selecting an empty pi/omp CLI default removes the remembered model and clears any persisted model-reselection warning.
- Refresh remains available for every non-ready probe state. It detects installs added to existing PATH directories; restart is only needed when PATH itself changed.

## 6. Bugs found by review (all fixed, all regression-tested)

The review loops caught and regression-tested:
1. Nonzero CLI exits could previously look successful or lose the CLI's useful guidance. They now always fail while preserving a bounded sanitized detail.
2. Output was truncated before validation, allowing a 4097-byte response to masquerade as a valid 4096-byte result. Truncation is now observable, retried once, then rejected.
3. PATH resolution stopped at the first unsafe Windows shim. It now searches all candidates and accepts only native `.exe` launchers on Windows.
4. CLI subprocesses had independent waits that could exceed the advertised timeout. One deadline now covers write, wait, drains, termination, and reap.
5. Guided setup could open the API-key modal for a CLI provider; CLI readiness is now probe-controlled.
6. Selecting pi/omp's empty CLI default could revive an old remembered model or leave a migrated reselection warning persisted. Both states are now cleared.
7. An outer executor deadline could drop the CLI future before its internal cleanup ran. Every spawned group/job is now guarded so cancellation synchronously kills the group and schedules reaping; a regression proves a descendant cannot survive a dropped future.

## 7. What remains (in priority order)

1. **Founder macOS smoke** — successful default and non-default polish for Claude/pi/omp, shell-metacharacter dictation, logout/error fallback and toast, cancellation/timeout with no child left, and Advanced picker interaction. Automated model-list smoke passed for pi and omp; the local Claude account currently returns `403 subscription disabled`, and the automated native-window run rendered blank, so neither substitutes for this smoke.
2. **Physical Windows smoke** — native `.exe` resolution, rejection of script-only installs, later-safe PATH candidate selection, install-then-Refresh behavior, literal shell metacharacters, no console flash, and no child after timeout.
3. **Merge to main** — only after the founder macOS and physical Windows smoke. Founder decides. The agent-CLI install, sign-in, Refresh/restart, and provider-choice copy was founder-approved on 2026-08-04.
4. Optional post-merge: OpenRouter $/M cost display and opt-in browser URL context.

## 8. Founder context (how to work with him)

- Voice-dictates messages — expect loose phrasing; extract intent, confirm only real ambiguities.
- Wants momentum: don't pause at green checkpoints; do pause for product decisions and anything user-facing (copy!).
- Will challenge unverified claims ("are you sure?") — bring evidence, not vibes. Two claims died on this branch that way (MSIX gating, warm sessions).
- Default English for transcription everywhere; auto-detect language intentionally NOT offered.
- Environment note: Codex CLI stop-review gate failed repeatedly on 2026-07-10 → root cause was an outdated Codex CLI (0.142.5) + stale broker daemons holding the old binary; fixed by `codex update` (→0.144.1) and killing brokers. If the gate flaps again with empty output, check `codex exec` directly and broker process age.
