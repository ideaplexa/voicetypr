# Phase 4B — App-Context Awareness: implementation spec

Status: **READY FOR IMPLEMENTER** (spec verified against current code 2026-07-05, worktree `feat/ai-polish-personalization`). Implementer TBD (founder deciding: GLM 5.2 / Codex / Claude). Claude gates + reviews. Companion to plan `030-ai-polish-personalization.md` §12 (4B).

Goal: make Polish aware of WHICH app the user dictates into (Slack=chat → no trailing period, casual; code editor → keep identifiers; email → structured prose) by feeding an app→category hint into the polish prompt. **Local-only + transparent** (history shows the category applied). A partial seam exists but the captured app identity never reaches the prompt today.

## Seam (current code)
- **Gate blocking 4B:** `app_rules_need_active_app` (`writing/pipeline.rs:31-33`) → capture fires ONLY if ≥1 enabled per-app rule exists. `capture_active_app_context` (`pipeline.rs:231-244`) stores `app_name` ONLY, discarding title/process_path. Called at `pipeline.rs:159-160` and `:388-389`.
- **`ContextHint`** (`writing/settings.rs:185-189`): only `app_name: Option<String>`. `active_win_pos_rs 0.10.1` `ActiveWindow` also exposes `title`, `process_path: PathBuf`, `process_id`, `window_id`, `position`. **No real bundle id** — macOS `process_path` = `bundleURL().path()` (e.g. `/Applications/Slack.app`); classify on `app_name` (`kCGWindowOwnerName`) + `.app`/`.exe` stem. macOS `title` (`kCGWindowName`) needs Screen-Recording permission — treat title as best-effort; classify off `app_name`/`process_path`.
- **Prompt path (key finding):** app identity does NOT reach the prompt today — it only drives preset selection. `run_smart_formatting` (`pipeline.rs:292-308`) builds `ai_context` = a vocabulary/term list (`smart_formatting_ai_context` `:269-278` → `compile_context_for_target` `vocabulary.rs:75-112`), passed as `context` to `polish_text_typed` (`ai.rs:1088-1104`) → `build_enhancement_prompt_for_transcript_language` (`prompts.rs:220-265`), injected only at `:257-262` ("Known terms …"). The `context` param = vocabulary, nothing about the app.
- **Per-app rules:** `AppFormattingRule { app_name, preset, enabled }` (`settings.rs:90-96`); `matching_app_formatting_preset` (`pipeline.rs:64-80`) substring-matches lowercased active app_name → preset; applied in `resolve_pipeline_config` (`pipeline.rs:82-109`, override at `:90-92`).
- **Transparency:** `WritingResult.context_hint` (`settings.rs:212`, set `pipeline.rs:528`) → `build_writing_history_metadata` (`audio.rs:1090-1144`, serialized `:1131-1134`). Category inside `ContextHint` flows to history automatically. Frontend type `src/types.ts:124`, filter `RecentRecordings.tsx:155,:282`, badge `:832-836`.

## Design
- **Taxonomy storage = static Rust module** `src-tauri/src/writing/app_category.rs` (NOT JSON — small, developer-curated, behavior-coupled with the `*_TRANSFORM` prompt consts; zero I/O, no parse-failure path, trivially unit-testable). Export via `writing/mod.rs`.
- **`AppCategory` enum** (serde snake_case): `Chat, Email, Docs, Code, Terminal, Social, Notes, Browser, Other`.
- **`classify(&ContextHint) -> AppCategory`**: match lowercased `app_name`, then `process_path` stem. Curated substrings — Chat: slack/discord/telegram/whatsapp/messages/signal/teams · Email: mail/outlook/spark/airmail/thunderbird/superhuman · Code: code/cursor/xcode/intellij/pycharm/webstorm/zed/sublime/android studio/nova · Terminal: terminal/iterm/warp/alacritty/kitty/wezterm/powershell/cmd/windowsterminal · Docs: word/pages/google docs/confluence/libreoffice · Social: x/twitter/mastodon/reddit/linkedin/bluesky · Notes: notes/obsidian/bear/craft/logseq · Browser: safari/chrome/firefox/arc/edge/brave (neutral hint) · else Other.
- **`category_prompt_hint(AppCategory) -> Option<&'static str>`** (None for Browser/Other): Chat "Keep it casual… no trailing period on a short single sentence… preserve emoji/@mentions." Email "Structure as clear prose, proper punctuation, professional-but-natural." Docs "Well-structured prose, full punctuation, paragraph breaks on pauses." Code "Keep identifiers, camelCase/snake_case, symbols exactly as spoken; no prose padding." Terminal "Literal command line: no capitalization changes, no trailing period, preserve flags/paths." Social "Short, punchy, casual; preserve hashtags/@handles/emoji; no trailing period on a one-liner." Notes "Terse note style; keep fragments as fragments."
- **Precedence (single decision point = `resolve_pipeline_config`):** explicit matching `AppFormattingRule` → use rule preset AND `category_hint = None` (user's explicit rule wins; suppress the auto nudge). No matching rule → global preset AND `category_hint = Some(classify(active_app))`. Store `category_hint` on `EffectiveConfig` (`pipeline.rs:51-56`).
- **Prompt injection:** new param `app_category_hint: Option<&str>` on `build_enhancement_prompt_for_transcript_language`, pushed as its own paragraph AFTER the mode transform (`prompts.rs:~246`) and BEFORE the "Known terms" block (`:257`): `"\n\nYou are dictating into a {label} context. {hint}"`. Composes: mode = structural, category = behavioral nudge, terms = spelling. Keep short so it never overpowers the mode transform.

## File-change checklist
1. **Loosen gate** — `pipeline.rs:159-160` & `:388-389`: `app_rules_need_active_app(&settings) || ai_enabled` (capture whenever polish may run; one `get_active_window()` call).
2. **Extend `ContextHint`** (`settings.rs:185-189`): add `#[serde(default)] window_title`, `process_path`, `category: Option<AppCategory>`. Populate window_title/process_path in capture (`pipeline.rs:241-243`). Update test ctor `audio.rs:2267-2269`.
3. **New module** `writing/app_category.rs` (`AppCategory`, `classify`, `category_prompt_hint`, `category_label`); export in `writing/mod.rs`.
4. **Decision** — `resolve_pipeline_config` (`pipeline.rs:82-109`): add `category_hint` to `EffectiveConfig`; `None` when `app_preset.is_some()`, else `classify`. Stamp resolved category onto `active_app.context_hint.category` in `process_transcription` before building `WritingResult` (`:520-528`).
5. **Thread hint into prompt** — add param to `build_enhancement_prompt_for_transcript_language` (`prompts.rs:220`), thread through `polish_text_typed` (`ai.rs:1088-1104`) + `run_smart_formatting`/`SmartFormattingRequest` (`pipeline.rs:280-308`), passing `config.category_hint.map(category_prompt_hint)`. Update the `#[cfg(test)]` shim `prompts.rs:211-218`.
6. **Transparency** — category auto-serializes via `build_writing_history_metadata` (`audio.rs:1131`). Frontend: `types.ts:124` → `{ app_name?; category? }`, category badge near `RecentRecordings.tsx:832-836`, optional category facet.

## PRIVACY (load-bearing — matches founder's local+transparent mandate)
- **Local-only:** no new network calls; classification is pure-local string matching.
- **`window_title` is potentially sensitive** (document names, DM subjects). Capture it for classification if useful, but **EXCLUDE `window_title` from the history-serialized `ContextHint`** (or gate it). The existing guard test `build_writing_history_metadata_uses_safe_fields_only` (`audio.rs:1701`) must stay green — extend it to assert `window_title` is NOT written to history.
- Explicit per-app rule ALWAYS wins over category default. Category is additive to the prompt, never replaces the mode transform.

## Test plan
1. `classify` mapping (macOS `app_name` + Windows path-stem inputs): Slack→Chat, VS Code/`Code.exe`→Code, Mail/Outlook→Email, Terminal/iTerm→Terminal, Obsidian→Notes, unknown→Other.
2. `category_prompt_hint`: Some/non-empty for Chat/Email/Docs/Code/Terminal/Social/Notes; None for Browser/Other.
3. Prompt-injection presence (mirror `ai/tests.rs:186-197`): with `Some(chat_hint)` prompt contains the hint + "dictating into a Chat context"; with `None` contains neither; category paragraph precedes the "Known terms" block.
4. Precedence (`resolve_pipeline_config`, reuse fixtures `pipeline.rs:636-935`): explicit rule → preset=rule & category_hint=None; recognizable app, no rule → global preset & category_hint=Some.
5. No-app fallback: `active_app==None` → category_hint None; empty app_name / `capture(false)` → None.
6. Transparency+privacy (`audio.rs`, extend `:1701`): `context_hint.category=Some(Chat)` serializes `"chat"` into history; **`window_title` NOT serialized**; category absent when context_hint None.

Gate: `pnpm typecheck && pnpm lint && pnpm test && (cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings)`.
