# Plan 081 Q3 catalog audit (2026-10-03)

Q3 refreshes catalog/default selection and provider request options. Q2 guard and skip behavior is unchanged. No commit, push or live provider calls.

| Provider | Before | After | Added | Removed |
|---|---:|---:|---:|---:|
| anthropic | 25 | 16 | 5 | 14 |
| gemini | 20 | 19 | 6 | 7 |
| openai | 49 | 36 | 10 | 23 |
| openrouter | 5 | 4 | 4 | 5 |

99 → 75 models; 25 added, 49 removed. Previous SOURCE.md counted only the 94 native models, omitting the 5 curated OpenRouter entries. Snapshot SHA256: `3e83335a8143df2043a2f07c98d0a68f7c90089f785202b8728e1c8e5a9d2b79`.

## Removed IDs

- anthropic: `claude-3-5-haiku-20241022`, `claude-3-5-haiku-latest`, `claude-3-5-sonnet-20240620`, `claude-3-5-sonnet-20241022`, `claude-3-7-sonnet-20250219`, `claude-3-haiku-20240307`, `claude-3-opus-20240229`, `claude-3-sonnet-20240229`, `claude-opus-4-0`, `claude-opus-4-1`, `claude-opus-4-1-20250805`, `claude-opus-4-20250514`, `claude-sonnet-4-0`, `claude-sonnet-4-20250514`
- gemini: `gemini-2.0-flash`, `gemini-2.0-flash-lite`, `gemini-2.5-flash-image`, `gemini-3-pro-image-preview`, `gemini-3-pro-preview`, `gemini-3.1-flash-image-preview`, `gemini-3.1-flash-lite-preview`
- openai: `chatgpt-image-latest`, `gpt-3.5-turbo`, `gpt-4`, `gpt-4-turbo`, `gpt-4.1-nano`, `gpt-4o-2024-05-13`, `gpt-5-chat-latest`, `gpt-5-codex`, `gpt-5.1-chat-latest`, `gpt-5.1-codex`, `gpt-5.1-codex-max`, `gpt-5.1-codex-mini`, `gpt-5.2-chat-latest`, `gpt-5.2-codex`, `gpt-5.3-chat-latest`, `gpt-image-1-mini`, `gpt-image-1.5`, `o1`, `o1-pro`, `o3-deep-research`, `o3-mini`, `o4-mini`, `o4-mini-deep-research`
- openrouter: `anthropic/claude-3-haiku`, `google/gemini-2.5-flash-lite`, `meta-llama/llama-3.3-70b-instruct`, `openai/gpt-4.1-nano`, `openai/gpt-4o-mini`

## Recommendations (primary first)

- anthropic: `claude-sonnet-5-5`, `claude-opus-5-5`
- gemini: `gemini-3.8-flash`, `gemini-3.5-flash-lite`
- openai: `gpt-6-luna`, `gpt-6.1-sol`
- openrouter: `openai/gpt-6-luna`, `google/gemini-3.8-flash`, `anthropic/claude-sonnet-5.5`, `google/gemini-3.5-flash-lite`

All ten recommendations are verified in the projected snapshot. OpenRouter labels, costs, limits, sampling flags and reasoning options are copied from its snapshot, with runtime `openai_compatible`. Native providers keep every upstream available text-input/text-only-output model, excluding deprecated/retired entries. OpenRouter remains curated.

## Reasoning handling

- OpenAI Luna: `reasoning_effort: none`; Sol: `low`. GPT-6 uses `max_completion_tokens`, with unsupported temperature omitted.
- Gemini Flash: `thinkingLevel: LOW`; Flash Lite: `MINIMAL`. No legacy thinking budget is sent for these models.
- Anthropic Sonnet: `thinking: between_tools` and `output_config.effort: low`; Opus: `adaptive` and `low`. No sampling parameters or obsolete budget fields.
- OpenRouter: explicit `reasoning.effort` from each model’s advertised floor (`none`, `low`, `low`, `minimal` in recommendation order), with thinking excluded from the response.
- Supported explicit request reasoning overrides are honored; tests cover `medium` for every recommendation. The existing Advanced UI is local-agent-only; no settings/schema keys changed.
- Claude Code uses `--effort low` where supported and a subprocess-only `CLAUDE_CODE_EFFORT_LEVEL` override so inherited high effort cannot win. Supported Advanced levels override that low default. CLI `off` maps to low rather than omitting the effort flag. pi/omp retain `--thinking off`; fast mode remains independent of reasoning.

Claude thinking rules: [Anthropic Messages documentation](https://platform.claude.com/docs/en/build-with-claude/thinking). Claude Code effort control: [Claude Academy](https://academy.claude.com/tutorials/choosing-the-right-effort-level-in-claude-code). The narrow current-Claude Messages path is necessary because genai 0.6 has legacy Claude model tables and emits fields rejected by Claude 5.5.

## Claude Code and stale selections

Choices are `fable` (Fable 5.1), `opus` (Opus 5.5), and `sonnet` (Sonnet 5.5), beside the Default entry. Empty/default selections explicitly invoke Sonnet (best measured quality and latency). Saved `haiku` remains accepted but is absent from choices. pi and omp have no hard-coded application model default: defaults are discovered from the installed CLI. Invalid defaults are omitted from choices; invalid saved selectors/defaults produce an actionable reselection message before a completion is launched. This adds discovery work to pi/omp invocations; live latency is NEEDS-SMOKE.

Startup migration now validates against all available catalog models rather than recommendations only. It preserves supported old models and CLI aliases; removed active IDs still clear the active model and set the existing reselection flag. Request dispatch also resolves removed catalog IDs to the primary recommendation, covering the CLI and provider model-memory paths. Custom endpoints retain their own namespaces. Tests cover saved OpenAI nano, Gemini Flash Lite, Anthropic Haiku and stale pi/omp selections.

## Remaining old-id hits

Every matching line from the requested git grep audit, plus matching new untracked source files, appears below. Catalog matches are intentional backward-compatible upstream selections/metadata; removal records, migration tests and Q2 historical fixture paths are retained for their stated purposes.

- `src-tauri/catalog/SOURCE.md:26` - openai: `gpt-3.5-turbo`, `gpt-4o-2024-05-13`, `o4-mini`, `o3-mini`, `gpt-4`, `gpt-4.1-nano`, `gpt-5.2-chat-latest`, `o1`, `gpt-image-1`, `gpt-5.3-chat-latest`, `gpt-4-turbo`, `o1-pro` — Removal record: upstream deprecated entries, excluded from shipped catalog.
- `src-tauri/catalog/catalog.generated.json:214` "model_id": "claude-sonnet-4-5", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:226` "model_id": "claude-sonnet-4-5-20250929", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:238` "model_id": "claude-sonnet-4-6", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:320` "model_id": "gemini-2.5-computer-use-preview-10-2025", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:332` "model_id": "gemini-2.5-flash", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:344` "model_id": "gemini-2.5-flash-lite", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:356` "model_id": "gemini-2.5-pro", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:612` "model_id": "gpt-4.1", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:624` "model_id": "gpt-4.1-mini", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:701` "model_id": "gpt-5-mini", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:718` "model_id": "gpt-5-nano", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:87` "model_id": "claude-haiku-4-5", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/catalog.generated.json:99` "model_id": "claude-haiku-4-5-20251001", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:102` "family": "claude-haiku", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:103` "id": "claude-haiku-4-5", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:132` "claude-haiku-4-5-20251001": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:139` "family": "claude-haiku", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:140` "id": "claude-haiku-4-5-20251001", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:1515` "gpt-4.1": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:1522` "id": "gpt-4.1", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:1545` "gpt-4.1-mini": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:1552` "id": "gpt-4.1-mini", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:1734` "gpt-5-mini": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:1741` "id": "gpt-5-mini", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:1775` "gpt-5-nano": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:1782` "id": "gpt-5-nano", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:477` "claude-sonnet-4-5": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:485` "id": "claude-sonnet-4-5", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:514` "claude-sonnet-4-5-20250929": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:522` "id": "claude-sonnet-4-5-20250929", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:551` "claude-sonnet-4-6": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:559` "id": "claude-sonnet-4-6", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:699` "gemini-2.5-computer-use-preview-10-2025": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:719` "id": "gemini-2.5-computer-use-preview-10-2025", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:741` "gemini-2.5-flash": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:749` "id": "gemini-2.5-flash", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:784` "gemini-2.5-flash-lite": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:792` "id": "gemini-2.5-flash-lite", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:827` "gemini-2.5-pro": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:850` "id": "gemini-2.5-pro", — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/catalog/models.dev.snapshot.json:95` "claude-haiku-4-5": { — Backward-compatible upstream metadata/selection: still available and not recommended.
- `src-tauri/src/ai/catalog.rs:240` resolve_model("openai", "gpt-4.1-nano").as_deref(), — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog.rs:244` resolve_model("openrouter", "google/gemini-2.5-flash-lite").as_deref(), — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog.rs:249` resolve_model("anthropic", "claude-haiku-4-5").as_deref(), — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog.rs:250` Some("claude-haiku-4-5") — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog.rs:253` resolve_model("gemini", "gemini-2.5-flash-lite").as_deref(), — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog.rs:254` Some("gemini-2.5-flash-lite") — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog_migration_tests.rs:10` ("openrouter", "google/gemini-2.5-flash-lite", false), — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog_migration_tests.rs:7` ("openai", "gpt-4.1-nano", false), — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog_migration_tests.rs:8` ("gemini", "gemini-2.5-flash-lite", true), — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/catalog_migration_tests.rs:9` ("anthropic", "claude-haiku-4-5", true), — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/ai/polish_regression_tests.rs:75` fixture: Some(repo.join("perf-corpus/polish/fixtures/claude-haiku-baseline.jsonl")), — Historical Q2 recorded-response fixture path; retained guard/skip evidence.
- `src-tauri/src/ai/polish_regression_tests.rs:8` read_jsonl(&repo.join("perf-corpus/polish/fixtures/claude-haiku-baseline.jsonl")).unwrap(), — Historical Q2 recorded-response fixture path; retained guard/skip evidence.
- `src-tauri/src/ai/runtime_tests.rs:135` model: "gpt-4.1-nano", — Intentional stale-selection/backward-compatibility test.
- `src-tauri/src/lib.rs:2341` "gemini-2.5-flash", — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.
- `src-tauri/src/lib.rs:2342` json!({ "google": "gemini-2.5-flash" }), — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.
- `src-tauri/src/lib.rs:2349` json!({ "gemini": "gemini-2.5-flash" }) — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.
- `src-tauri/src/lib.rs:2351` assert_eq!(values["ai_model"], json!("gemini-2.5-flash")); — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.
- `src-tauri/src/lib.rs:2359` "gemini-2.5-flash", — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.
- `src-tauri/src/lib.rs:2360` json!({ "google": "gemini-3-flash-preview", "gemini": "gemini-2.5-flash" }), — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.
- `src-tauri/src/lib.rs:2366` json!({ "gemini": "gemini-2.5-flash" }) — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.
- `src-tauri/src/lib.rs:2461` "gemini-2.5-flash", — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.
- `src-tauri/src/lib.rs:2462` json!({ "google": "gemini-2.5-flash" }), — Intentional migration test: verifies preservation of available old settings or reselection for removed IDs.

## Verification

- Final `cargo test`: 1,675 passed, 25 ignored; 2 CLI integration tests passed.
- Final `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Final `cargo fmt --check`: passed.
- `pnpm typecheck`: passed.
- Final frontend suite (`pnpm exec vitest run --maxWorkers=2`): 949 passed across 84 files. The normal full run also passed earlier; later concurrent runs intermittently failed the unchanged EnhancementSettings dropdown test, which passed in isolation and in the final reduced-concurrency suite.
- Repeated snapshot projection and catalog generation are byte-identical. Filter fixtures verify text input, text-only output and deprecated exclusion. All recommendation labels, costs, membership and order match the snapshot/overlay.
- `git diff --check`: passed. Changes are uncommitted on `feat/polish-quality-p1`.

Live API/CLI requests, pi/omp discovery latency and native dictation acceptance remain **NEEDS-SMOKE**. The Rust build reused the existing sidecar state (cached build warning: Swift build script not found); no new sidecar build or native smoke is claimed. No build script was edited or moved in Q3.
