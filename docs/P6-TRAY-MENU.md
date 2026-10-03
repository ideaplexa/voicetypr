# Plan 080 P6 native tray menu

The tray uses Tauri `MenuBuilder`, `Submenu`, `CheckMenuItem` and native separators.
macOS uses Title Case, Windows sentence case. Both use the same order: status
(and Fix when blocked), retained-recording retry/discard, Start/Stop, Polish,
Engine (local/cloud/network), Microphone, Language, Mode, Live Preview,
copy/paste/recent history, file transcription, app navigation/update/help, Quit.
Windows leading icons are omitted. Check for Updates stays visible but disabled
for store-managed installations, preserving their updater restriction. The primary shortcut is display text in the
label: Tauri's menu accelerator API registers an accelerator rather than offering
an independent display-only field. Settings and macOS Quit use real native
accelerators. The effective primary recording shortcut comes from the same
resolver as the app, including native modifier bindings. The default for new
installs is two keys: ⌥ Space on macOS, Ctrl+Space on Windows (Alt+Space would
leave a lone Alt tap that opens app menus). Settings, the effective-primary
resolver and the trigger engine share that default; saved shortcuts are kept.

`menu/model.rs` builds the pure item tree. `menu/tray.rs` collects only cached
engine/server information, settings, devices and history, then renders that tree.
`menu/actions.rs` calls existing commands for model/microphone selection,
recording, Polish, clipboard, paste and retained audio. Language, mode and live
preview use `get_settings` + `save_settings`. Polish uses `get_ai_settings`,
`update_ai_settings` and `update_enhancement_options`; explicit global styles
now survive the existing backend normalization so they are effective in both
recording and the app. The app reloads Polish options on `polish-options-changed`.
No menu ID or transcript preview is logged.

Existing refresh triggers remain. Added triggers cover successful settings and
Polish persistence, cloud credentials, history changes, recording start/stop,
blocked status, retained audio creation/discard/retry/expiry. A settings save
does not claim a microphone or permission blocker was fixed; the next recording
attempt refreshes the P4 result, and successful license rechecking clears it. While recording,
a generation-scoped task changes only the native status item's text once a
second; it does not enumerate devices, read history or rebuild the menu.

Main-window navigation is a typed `main-navigate` payload:
`{ screen: ScreenId, pane?: SettingsPane | null, source?: SourceFilter | null }`.
`useAppEvents` handles it and P4's `island-navigate` through
`mainNavigation.ts`. Models opens Transcription/local, cloud keys opens
Transcription/cloud, and choose mic opens Recording. Per-app Styles opens
Polish; More opens Transcription; file transcription uses the existing `audio`
alias and upload dialog. Settings opens the existing Settings screen at the
requested pane. Insights opens Home, which contains the existing weekly stats.
The coordinator explicitly routes both navigation events to main even when the
island is the active window. Recovery navigation remains user initiated.

Language menus use the local catalog (including English-only models), cached
remote model identity and curated provider sets; large sets show the eight
common supported languages and More. Provider references checked 2026-10-03:
[Soniox](https://soniox.com/docs/stt/concepts/supported-languages),
[Cohere](https://docs.cohere.com/docs/transcribe),
[Deepgram](https://developers.deepgram.com/docs/models-languages-overview/).

## NEEDS-SMOKE

- macOS and Windows native menu appearance, keyboard navigation and screen reader.
- Real speech: Start/Stop, primary/native-modifier shortcut display, clock updates
  while a menu is open, cancel/auto-stop, and back-to-back recording.
- Every P4 blocker: status/Fix, successful remediation and subsequent recording.
- Kept recording: alternate-engine retry, discard, busy retry and expiry.
- Quick-setting changes from the tray and app stay synchronized with the island.
- Actual input devices/hotplug, LAN servers, cloud keys and provider languages.
- Copy/paste targets and cursor/focus preservation after closing the native menu.
- Main window hidden: every navigation target, repeated file-dialog invocation,
  native Settings/Quit accelerators, store-install update behavior.

No pill, preview or design files were changed. No commit, push or release.

## Validation receipt (2026-10-03, macOS)

- `cargo test`: 1,683 passed, 25 existing ignored, zero failed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --check`: passed.
- `pnpm typecheck` and `pnpm lint`: passed.
- `pnpm exec vitest run --maxWorkers=4`: 963 passed across 86 files.
- Added 12 Rust and 12 frontend tests; no tests removed. Updated two existing
  Rust assertions to preserve explicitly selected global Polish styles.
- SwiftPM could not use its nested sandbox and existing build caches under the
  agent sandbox. Rust gates used a temporary `swift` wrapper with
  `--disable-sandbox --build-system native`, writable cache/module-cache paths
  and `/tmp/p6-swift-build` scratch space. The real Swift sidecar was compiled
  and verified; repository build scripts were unchanged.
- No native menu, hardware, real-speech or Windows runtime smoke was performed.
