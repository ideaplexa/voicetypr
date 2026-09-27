# Plan 044 — Harden agent-CLI Polish and add model selection

Status: **IMPLEMENTED — automated verification, adversarial review, and founder copy approval passed; founder macOS and physical Windows smoke remain**
Priority: **P1 before Plan 030 merge**  
Planned at: `4dfebaaf` on 2026-08-04, with an existing dirty worktree (24 modified files)  
Effort: **M**  
Risk: **MEDIUM** — process execution, persisted settings, and three external CLI contracts

## Decision summary

Ship one hardening cut before the final Plan 030 review:

1. Restore real process isolation for Claude Code, pi, and omp.
2. Make process completion, timeout, output, and Windows executable handling fail closed.
3. Close the output-truncation hole so malformed model output always reaches the raw-transcript fallback.
4. Re-enable Refresh for newly installed CLIs and distinguish missing, incompatible, and unsafe launchers.
5. Add model selection under Advanced:
   - Claude Code: a small curated list because its CLI exposes `--model` but no machine-readable model-list command.
   - pi: discover models through its machine-readable RPC `get_available_models` command.
   - omp: discover models through `omp models --json --no-extensions`.
6. Keep reasoning non-configurable: pi/omp use `--thinking off`; Claude uses `--effort low` when supported and otherwise remains on the fast Haiku default.

Do **not** add a thinking selector. Polish is a bounded text transformation, not an agent reasoning task.

## Evidence gathered

### Repository evidence

- `src-tauri/src/ai/agent_cli.rs:162-204` currently launches pi/omp without all available isolation flags.
  - pi is missing `--no-extensions`, `--no-skills`, `--no-prompt-templates`, and `--no-context-files`.
  - omp is missing `--no-extensions`, `--no-lsp`, and `--no-title`.
- `agent_cli.rs:299-302` calls `current_dir(std::env::temp_dir())`; the shared system temp directory is not the documented empty working directory.
- `agent_cli.rs:326-332` discards stdin write failures.
- `agent_cli.rs:345-366` parses stdout after a nonzero exit and can accept an apparently valid assistant message as success.
- `agent_cli.rs:604-620` calls `which::which_in` and rejects only the first result. A rejected shim prevents discovery of a later safe executable.
- `agent_cli.rs:632-637,1453-1465` deny only `.cmd`/`.bat`; tests explicitly allow `.ps1` and `.sh`, despite the surrounding comments promising native Windows executables.
- `src-tauri/src/ai/executor.rs:94-108,296-347` truncates before validating. For a short input, a runaway response is truncated to exactly 4096 bytes and then passes the `output_len > 4096` anomaly check.
- `src/components/sections/EnhancementsSection.tsx:1193-1198` says Refresh cannot detect a new install because PATH is cached. That is false when the executable is installed into an already-cached PATH directory: `resolve_binary()` searches the cached directories again on every probe.
- `src-tauri/src/commands/ai.rs:1325-1337` deliberately returns an empty model list for agent-CLI providers even though pi and omp expose model discovery.
- `src-tauri/src/ai/contract.rs:21-27` already carries `model_id` through every polish request. No new request layer is needed.
- `ai.rs:201-229,768-779` already persists per-provider model choices. Empty CLI-default selection needs one correction: remove the old remembered entry instead of leaving it stale.

### Live CLI evidence

Verified on this workstation on 2026-08-04:

- Claude Code `2.1.220`
  - `claude --help` exposes `--model <model>`, `--effort <low|...>`, and `--safe-mode`.
  - It exposes no machine-readable model-list command.
  - `claude --safe-mode auth status` exited successfully while retaining subscription authentication. Do not record or surface the command's account fields.
- pi `0.83.0`
  - `pi --list-models` returned available models.
  - Its installed RPC contract defines JSON-lines command `{"type":"get_available_models"}` and returns `data.models` containing provider, id, context, and reasoning metadata.
  - `pi --help` confirms all required isolation flags.
- omp `17.2.7`
  - `omp models --json --no-extensions` returned structured models with `provider`, `id`, `selector`, `name`, context, reasoning/thinking, and cost metadata.
  - `omp --help` confirms `--model`, `--thinking off`, and all required isolation flags.

These listing operations made no model request and received no transcript.

### Runtime documentation

- Rust `Command::arg` passes ordinary arguments literally, but the standard library explicitly warns that `cmd.exe` and `.bat` files use unsafe non-standard argument decoding for untrusted input: <https://doc.rust-lang.org/std/process/struct.Command.html#method.arg>
- Tokio documents that child processes continue after handle drop by default; `kill_on_drop(true)` is required for cancellation-on-drop: <https://docs.rs/tokio/latest/tokio/process/struct.Child.html>
- `tempfile::TempDir` creates a randomly named directory and removes it when its owner drops: <https://docs.rs/tempfile/latest/tempfile/struct.TempDir.html>
- `which` 4.4.2 implements `which_in()` as the first item returned by `which_in_all()`. Filtering after `which_in()` therefore cannot reach a later safe executable: local crate source `which-4.4.2/src/lib.rs:151-159,199-215`.

## Invariants

The implementation is correct only if all of these remain true:

1. Dictated text never enters a shell command string.
2. Windows `.cmd`, `.bat`, PowerShell, WSH, and other script launchers never receive dictated text or the system prompt.
3. Every CLI runs with tools, extensions, project context, and unrelated agent features disabled as far as that CLI supports; preserve Claude's empirically proven persistence flags unless a live authenticated smoke explicitly validates a change.
4. Every polish process runs in a newly created empty directory whose lifetime covers the child process.
5. Timeout, canceled write, failed read, nonzero exit, malformed JSON, oversized output, and model refusal all produce an error and preserve the raw transcript.
6. A nonzero process exit can contribute a bounded user-facing error message, but never successful polished text.
7. Model discovery never sends a transcript or makes a model completion request.
8. Existing users with an empty agent-CLI model keep current behavior:
   - Claude Code → Haiku.
   - pi/omp → the CLI's configured default.
9. pi/omp always run with thinking off. Claude runs at low effort when the installed version supports it. No thinking UI is added.
10. Model discovery failure does not disable an already configured CLI default; it only prevents choosing a different model until Refresh succeeds.

## Scope

### Files expected to change

- `src-tauri/src/ai/agent_cli.rs`
- `src-tauri/src/ai/executor.rs`
- `src-tauri/src/commands/ai.rs`
- `src-tauri/src/ai/runtime_tests.rs`
- `src-tauri/src/ai/tests.rs` only if shared AI behavior tests belong there
- `src/types/providers.ts`
- `src/hooks/useProviderModels.ts` only if existing caching cannot represent CLI discovery errors
- `src/components/sections/EnhancementsSection.tsx`
- `src/components/sections/__tests__/EnhancementsSection.test.tsx`
- `CHANGELOG.md`
- `plans/030-HANDOFF.md` after behavior is verified

`tempfile` is already a normal dependency in `src-tauri/Cargo.toml`; do not add another temporary-directory crate.

### Explicitly out of scope

- Warm or persistent CLI sessions.
- A user-facing thinking/effort selector.
- Executing `.cmd`/`.bat` through `cmd.exe`.
- Parsing npm/bun batch shims to locate their underlying JavaScript entrypoint.
- OpenRouter cost UI.
- Browser URL context.
- New telemetry containing model names, transcript text, executable paths, or CLI account details.
- Unrelated rustfmt normalization in keytrigger, cloud STT, shortcuts, or trigger code.

## Worktree preparation

The plan was written while 24 files were already modified. Before editing:

```bash
git status --short
git diff --stat
git diff -- src-tauri/src/ai/agent_cli.rs src-tauri/src/ai/executor.rs \
  src-tauri/src/commands/ai.rs src/components/sections/EnhancementsSection.tsx
```

Do not reset, stash, overwrite, or reformat unrelated user changes. Remove unrelated formatting noise from the functional patch, or keep it isolated for a separate formatting-only commit. Do not commit or push without explicit user instruction.

STOP if the substantive existing changes described in “Repository evidence” are absent or materially different.

## Implementation plan

### 1. Make the CLI specification express capabilities and model policy

Refactor `AgentCliSpec` in `src-tauri/src/ai/agent_cli.rs` so provider policy is data rather than scattered conditionals. It must describe:

- executable name;
- required isolation arguments;
- input mode and output parser;
- auth probe mode;
- model-discovery mode: `ClaudeCurated`, `PiRpc`, or `OmpJson`;
- default model: Claude `Some("haiku")`, pi/omp `None`;
- fixed reasoning policy: Claude low effort, pi/omp thinking off.

Target invocation contracts:

```text
Claude Code:
claude --safe-mode -p --tools "" --strict-mcp-config --no-chrome
  --model <selected-or-haiku> [--effort low when supported]
  --system-prompt <prompt> --output-format json

pi:
pi -p --no-tools --no-session --no-extensions --no-skills
  --no-prompt-templates --no-context-files --thinking off
  [--model <provider/id>] --mode json --system-prompt <prompt>

omp:
omp -p --no-tools --no-lsp --no-session --no-extensions --no-skills
  --no-rules --no-title --thinking off [--model <selector>]
  --mode json --system-prompt <prompt> <dictation>
```

Claude compatibility rule:

- Detect supported flags from bounded `--help` output once per resolved binary/version.
- Prefer `--safe-mode`; retain the existing `--setting-sources ""` isolation only as a compatibility fallback when `--safe-mode` is absent.
- Add `--effort low` only when advertised. When absent, keep the default model at Haiku and do not expose an effort setting.

pi/omp compatibility rule:

- Their required no-extension/no-context flags are security requirements, not optional enhancements.
- If the installed CLI does not advertise them, mark the probe incompatible and tell the user to update rather than silently launching with user customizations.

Add exact-argv tests for every provider, including selected model and empty-model behavior.

### 2. Use a real empty working directory and fail-closed process I/O

In `cold_spawn_and_collect` and auth/model-discovery subprocesses:

- Create `tempfile::TempDir` with a VoiceTypr prefix.
- Keep the `TempDir` owner alive until the child has exited or been killed.
- Set `current_dir(temp_dir.path())`.
- Treat stdin `write_all` failure as `AiProviderError::Internal`; close stdin after a successful write.
- Capture stdout and stderr concurrently with bounded collectors that continue draining after the retained-size cap, preventing both memory growth and pipe deadlock.
- Retain `kill_on_drop(true)`.
- On timeout, explicitly request kill and await process termination when possible before returning `Timeout`.

Exit contract:

- Exit 0: parse stdout normally.
- Nonzero exit: structured stdout/stderr may become a sanitized, control-free, maximum-200-character `AgentCli` message.
- Nonzero exit must never return `Ok`, even if stdout contains an assistant message before the failure.

Extract the process-result decision into a pure function so tests can cover status + stdout + stderr without launching real CLIs.

### 3. Correct executable resolution and probe states

Replace first-match post-filtering with `which::which_in_all`:

- Iterate every candidate in PATH order.
- Unix: accept regular executable files as `which` already validates.
- Windows: accept only native executable forms approved by the implementation (`.exe`, and `.com` only if explicitly justified/tested). Reject all script launchers, not only `.cmd`/`.bat`.
- Continue after a rejected candidate so a later native executable can win.

Return a typed internal result instead of `Option<PathBuf>`:

```rust
enum BinaryResolution {
    Ready(PathBuf),
    Missing,
    UnsafeLauncher,
}
```

Do not send absolute paths to the frontend. Extend `AgentCliProbe` with a compatibility state or equivalent boolean so the UI can distinguish:

- not installed;
- installed through an unsafe/unsupported launcher;
- installed but CLI version lacks mandatory isolation capabilities;
- installed and ready;
- installed but not authenticated.

Keep Refresh in every non-ready state. Correct the current restart-only copy: Refresh can see files added to an existing PATH directory; restart is a fallback only when PATH itself changed.

### 4. Close the output truncation hole

Change `sanitize_ai_output` so truncation is observable by the caller, for example:

```rust
struct SanitizedOutput {
    text: String,
    truncated: bool,
}
```

A truncated output is invalid. It should participate in the existing one-retry policy and then return `BadResponse`, causing the raw-transcript fallback. Never validate a truncated prefix as complete text.

Required tests:

- short input + 4097-byte response is rejected, not accepted as a 4096-byte result;
- short input + much larger response is rejected;
- multibyte truncation boundary never panics or emits invalid UTF-8;
- first response truncated, second valid → success after exactly one retry;
- both responses truncated → `BadResponse` after exactly two calls.

### 5. Add backend model discovery without model calls

Add `agent_cli::list_models(provider_id)` returning a small internal model DTO. Reuse `list_provider_models`; do not add a parallel frontend command.

#### Claude Code

Return a curated list because Claude Code has no machine-readable listing command:

- `haiku` — recommended/default, fast;
- `sonnet` — balanced;
- `opus` — highest quality.

Do not claim these are dynamically discovered. Keep the list deliberately curated: Haiku is the existing live-tested default, while the installed Claude help documents Sonnet and Opus aliases. Never probe support by making a paid model request.

#### pi

Spawn pi in isolated RPC mode with the same no-extension/no-context guarantees. Send one JSON line:

```json
{"id":"voicetypr-models","type":"get_available_models"}
```

Parse the matching response's `data.models`; then terminate and reap the RPC child. Convert each model to:

- selection id: `<provider>/<id>` (accepted by pi `--model`);
- display name from model name or humanized id;
- source provider for grouping;
- context/reasoning metadata when present;
- no subscription-cost claim.

#### omp

Run:

```text
omp models --json --no-extensions
```

Parse `.models[]` and use `selector` as the exact value passed to `omp --model`. Preserve `provider`, `name`, context, and reasoning metadata. Do not expose catalog token prices as the user's subscription cost.

#### Default entries

- Claude's recommended entry is Haiku.
- pi/omp prepend a synthetic “CLI default” entry represented internally by an empty model id and an explicit `cliDefault`/`cli_default` flag.
- Extend `ProviderModel`/`AIProviderModel` with optional source-provider and CLI-default fields rather than persisting a magic sentinel string.

Model-list subprocesses must have a short timeout, no transcript input, bounded output, and no model completion request.

### 6. Pass the selected model into every CLI invocation

Use the existing `AiPolishRequest.model_id`:

- Claude empty model → `haiku`; nonempty → selected alias/id.
- pi/omp empty model → omit `--model`, preserving the CLI's configured default.
- pi/omp nonempty model → append `--model` and the discovered exact selector as separate argv values.

Keep model value and prompt value as discrete `Command::arg` values.

Update `remember_provider_model` so selecting an empty CLI-default model removes the previous provider entry. Otherwise an old remembered model silently returns after restart.

Keep `selection_meets_model_requirement` compatible with empty agent-CLI defaults.

### 7. Reuse the existing Advanced model-picker UI

In `EnhancementsSection.tsx`:

- Show the model picker for a ready agent-CLI provider instead of excluding all agent CLIs.
- Fetch agent-CLI models on demand through the existing `useAllProviderModels` path.
- Keep the simple guided flow unchanged: selecting a CLI immediately enables Polish with its recommended/default model.
- In Advanced, group pi/omp models by their source provider so a large list remains navigable.
- Show the selected model for agent-CLI providers.
- Keep model Refresh available.
- Keep authentication Refresh available in every non-ready state.
- Do not add a thinking selector. A small non-interactive note that Polish keeps reasoning low/off is acceptable only after copy approval.
- Never show “Add API key” or “No models available” while discovery is still loading or failed; use the existing loading/error/retry states.

Do not redesign the section. Extend its existing model groups, buttons, loading state, and persistence path.

### 8. Update documentation only after runtime proof

After implementation and smoke tests:

- Update `plans/030-HANDOFF.md` with the final invocations, model-discovery behavior, Windows limitation, and smoke requirements.
- Remove stale comments in `agent_cli.rs` describing a future warm manager or Claude-only phase.
- Add a concise `CHANGELOG.md` entry under the current unreleased section.
- Any new user-facing copy requires explicit founder approval and must not use “Whisper.”

## Test plan

### Rust unit/integration tests

Add or update tests covering:

1. Exact isolation argv for all three CLIs.
2. Claude safe-mode/effort capability parsing and compatibility fallback.
3. Empty and selected model argv for all providers.
4. pi RPC model-response parsing, response-id matching, malformed response, timeout, and empty list.
5. omp JSON model parsing, selector preservation, malformed response, and empty list.
6. Windows native allowlist, unsafe script rejection, and later-safe-candidate selection.
7. Probe state mapping without leaking executable paths.
8. Nonzero exit + assistant stdout never succeeds.
9. Nonzero exit + structured error returns a capped sanitized CLI message.
10. stdin write failure returns an error.
11. unique temporary cwd lifetime.
12. oversized/truncated output retry and final fallback.
13. per-provider model persistence, including clearing a remembered model when “CLI default” is selected.

Use pure parser/decision helpers wherever possible. Tests must not require installed CLIs, credentials, network access, or a model call. Keep the existing real-CLI tests ignored and use them only for manual smoke.

### Frontend tests

Extend `src/components/sections/__tests__/EnhancementsSection.test.tsx`:

1. Ready CLI shows model picker in Advanced.
2. Claude shows curated Haiku/Sonnet/Opus and defaults to Haiku.
3. pi/omp show CLI default plus discovered, provider-grouped models.
4. Selecting a CLI model persists provider/model and updates selected state.
5. Selecting CLI default persists an empty model and clears the old remembered selection.
6. Model discovery error shows Retry, not API-key UI or a false empty-state.
7. Not-installed CLI still has Refresh.
8. Unsafe/incompatible launcher shows the correct non-secret state.
9. No thinking/effort selector is rendered.

## Verification gates

Run targeted checks first:

```bash
cd src-tauri
cargo test agent_cli --lib
cargo test sanitize_ --lib
cargo test ai_runtime_retries --lib
cargo test ai_runtime_gives_up --lib
cargo clippy --all-targets -- -D warnings

cd ..
pnpm test --run src/components/sections/__tests__/EnhancementsSection.test.tsx \
  src/components/RecordingPill.test.tsx
pnpm typecheck
pnpm lint
```

Then the full repository gate:

```bash
pnpm quality-gate
```

Expected: every command exits 0; no ignored real-CLI smoke is counted as runtime proof.

## Mandatory runtime smoke

External CLI flags and auth behavior are not proven by unit tests.

### macOS, real logged-in installation

For each provider:

1. Probe and model listing complete without a model call.
2. Advanced shows the expected model list.
3. Default model produces a short, correct polish.
4. Select one non-default model and verify the emitted result reports that model id.
5. Dictate text containing shell metacharacters; it is treated as text.
6. Force a CLI error/logout; raw transcript is delivered and the bounded CLI guidance appears.
7. Start a request and trigger cancellation/timeout; confirm no child remains.

Additionally for Claude, directly verify the final safe-mode invocation on the logged-in subscription before replacing the empirically working command. If safe mode loses auth or loads customization, STOP and retain the proven `--setting-sources ""` path.

### Windows, physical machine

1. Native `.exe` installation resolves and runs.
2. A `.cmd`/`.bat`/PowerShell-only installation is reported as incompatible and is never executed.
3. When an unsafe candidate precedes a safe native executable on PATH, the later native executable wins.
4. Install into an existing PATH directory while VoiceTypr runs; Refresh detects it without restart.
5. Add a new PATH directory; Refresh may miss it, and restart guidance is accurate.
6. Dictated shell metacharacters remain literal.
7. No console window flashes and no child remains after timeout.

Windows CI remains compile-only; this physical smoke is merge-blocking.

## Verification record

Observed on 2026-08-04:

- `pnpm quality-gate` passed: TypeScript, ESLint, 52 frontend files (599 passed, 1 skipped), 1,288 Rust tests (13 ignored), and clippy with warnings denied.
- Targeted process tests passed, including an outer-future-drop regression proving a descendant cannot survive cancellation of the CLI command future.
- `pnpm tauri build --debug --bundles app --ci` produced and Developer-ID-signed `Voicetypr.app`; debug notarization was intentionally skipped because notarization credentials were not present.
- Real no-completion model discovery passed for pi and omp and returned nonempty exact selectors. Real omp default-model polish passed.
- Real Claude polish reached the authenticated service but returned the account's current `403 subscription disabled` response. Real pi polish hit the 9-second VoiceTypr deadline. These are not successful provider smoke results.
- The signed app launched under native automation, but its main window rendered blank, so the Advanced picker and end-to-end app flow were not visually verified.
- No physical Windows host was available. A macOS cross-check reached native dependency compilation but could not compile `ring` without Windows SDK headers; Windows runtime behavior remains unverified.
- Independent backend and frontend adversarial re-reviews found no remaining merge-blocking code findings.
- Founder approved the final agent-CLI install, sign-in, Refresh/restart, and provider-choice copy.
- After reconciling current `main` on 2026-08-06, `pnpm quality-gate` passed again: TypeScript, ESLint, 55 frontend files (605 passed, 1 skipped), 1,349 Rust tests (13 ignored), and clippy with warnings denied. Fresh backend/frontend adversarial review found and closed two integration regressions: recording cancellation now owns and aborts in-flight CLI Polish, and legacy implicit Stable update-channel values no longer override a prerelease build's Beta default.

## Acceptance criteria

- [x] Supported CLI argv disables extensions, tools, sessions, project context, and unrelated agent features.
- [ ] Claude completes subscription-backed polish with the final isolation flags.
- [x] pi/omp always receive `--thinking off`; Claude uses low effort when advertised.
- [x] No thinking selector exists.
- [x] Truncated output can never be accepted or pasted.
- [x] Nonzero CLI exit can never yield successful polished text.
- [x] Every process tree uses a unique temporary cwd and one bounded group-kill/reap deadline.
- [x] Windows script launchers are rejected before dictated input is passed.
- [ ] Refresh install detection is verified on a running physical Windows app.
- [x] Claude exposes curated model choices; pi/omp expose discovered model choices.
- [x] Empty legacy model settings preserve Haiku/CLI-default behavior.
- [x] Model discovery never sends transcript text or makes a model completion request.
- [x] Targeted tests and the full quality gate pass.
- [ ] Founder macOS and physical Windows smoke pass.
- [x] Final user-facing copy is founder-approved.
- [x] Final branch receives a fresh PR-style adversarial review after this patch is clean.

## STOP conditions

Stop and report instead of improvising if:

- Claude safe mode does not preserve subscription auth on a real logged-in installation.
- pi's RPC model protocol or omp's JSON schema differs materially from the evidence above.
- Model listing makes a paid/model completion request.
- Supporting Windows requires executing a batch/PowerShell/WSH script with dictated text.
- The fix requires storing or reading CLI credentials directly.
- The implementation requires a new thinking selector or warm process manager.
- Existing uncommitted user changes conflict with the in-scope files.
- User-facing copy needs a product decision that has not been approved.
