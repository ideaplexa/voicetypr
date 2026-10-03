# Plan 082 section A implementation handoff

Implemented A1–A9 on `feat/2.1-beta5`. No commit, push, release, or publication. Section C remains outside this change.

| Item | Behaviour and changed files |
| --- | --- |
| A1 | Live mic-silence notes have a separate enumerated kind and generation-scoped withdrawal. Speech withdraws them; stop, cancel, paste, errors and new takes retire live warnings. Live warnings display beside the recording island instead of waiting for idle. The finished-take digital-zero `mic_silent` note remains. `src-tauri/src/commands/audio.rs`, `src-tauri/src/recording/island.rs`, `src/types/island-events.ts`, `src/pill.tsx`, `src/pill/feedback.ts`, `src/pill/stack.ts`, `src/pill/stack-view.ts`, `src/pill/renderer.ts`, `src/pill/feedback.test.ts`. |
| A2 | Capture clears the old app context before attempting the new capture. Icon failures remain retryable; registering a changed path invalidates the image. An old lookup cannot populate the cache for a replacement path. `src-tauri/src/commands/audio.rs`, `src-tauri/src/state/app_state.rs`, `src-tauri/src/pill/icons.rs`. |
| A3 | Usage aggregation remembers the latest dated app path internally, registers only the displayed aggregate apps and serializes opaque icon keys. Insights loads icons using the same registry command, permitted for main and pill windows, with the pill's shared fallback colour. `src-tauri/src/commands/usage_stats.rs`, `src-tauri/src/pill/icons.rs`, `src/types/usage.ts`, `src/components/insights/WhereYouTalk.tsx`, new `UsageApplicationIcon.tsx` and `UsageApplicationIcon.test.tsx` in that directory. |
| A4 | A decoded app image has a transparent container with no shadow or clipping. An image error restores the coloured letter tile. `src/pill.css`, `src/pill/app-icon.ts`, `src/pill/app-icon.test.ts`. |
| A5 | A separate What's new dialog loads the installed version's GitHub release body through Rust with a five-second timeout, one-hour successful cache and 30-second failure cache. Safe Markdown skips raw HTML and images, limits links to HTTPS, and opens links via the existing opener command. Offline fallback links to the version's release page. Help and About use this dialog; the update announcement retains its own copy and gains “See what's new”. The workflow uses an existing release body or generated GitHub notes as the identical source for release body and updater notes. `.github/workflows/release.yml`, `package.json`, `pnpm-lock.yaml`, new `src-tauri/src/commands/release_notes.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs`, new `src/components/WhatsNewDialog.tsx` and `.test.tsx`, `src/components/UpdateAnnouncementDialog.tsx`, `src/components/tabs/SettingsTab.tsx` and `.test.tsx`, `src/components/sections/ReportProblemSection.tsx` and `__tests__/ReportProblemSection.test.tsx`. |
| A6 | The pane can shrink and hides horizontal overflow; pane children, input and select controls have width guards. `src/components/tabs/SettingsTab.tsx`, `src/components/settings/settings-modal.css`. |
| A7 | The recording preview uses the actual island renderer in static listening mode with the chosen anchor and app letter placeholder. It schedules no animation frames, timers or global input listeners. Shadow DOM isolates island styles from the main window. The edge-offset row has margin above its border. `src/components/sections/recording/RecordingIndicatorCard.tsx`, new `IslandPreview.tsx` and `IslandPreview.test.tsx` in that directory, `src/pill/renderer.ts`. |
| A8 | Test dictation hides after successful nonempty history, including takes outside the loaded history page through whole-history usage stats. No setting added. `src/components/tabs/OverviewTab.tsx`, `OverviewTab.test.tsx`. |
| A9 | The dictionary derives whether the editor is valid without setting state during render. The Actions table header matches the other header padding. `src/components/sections/DictionarySection.tsx`. |

No changes to `src/components/ui/*` or settings contracts. App paths stay inside backend aggregation/registry memory; no new paths, transcript text or window titles in events, logs or analytics.

## Automated gates

Passed:

- `pnpm typecheck`
- `pnpm lint`
- `pnpm exec vitest run`: **106 files, 1,261 tests passed**. Twelve frontend tests added; existing tests retained. Two old idle long-silence fixtures now use a general notice while new tests explicitly verify live-only silence behaviour.
- `pnpm build`
- `cd src-tauri && cargo fmt --check`
- `git diff --check`

Frontend regression coverage includes warning withdrawal for a matching take, stop/cancel/paste retirement, retention of the finished-take warning, long-silence retirement, late-warning rejection, real-image/fallback colour behaviour, offline Markdown fallback and unsafe markup, static preview with zero rAF scheduling, and test-dictation visibility for completed/failed and older takes.

Four new Rust tests cover context clearing before a failed capture, icon failure retry/path replacement/stale lookup, newest app path with an opaque serialized icon key, and missing/empty release-note fallback. **Rust tests and clippy were not run**, per the SwiftPM sandbox constraint; Claude must compile and run them.

Dependency installation required a writable temporary pnpm store. Gate commands used `pnpm_config_verify_deps_before_run=false` to prevent pnpm's automatic reinstall into a sandbox-blocked store. The temporary dependency backup was removed. Vite emits its existing config-loader and bundle-size warnings; jsdom emits canvas-not-implemented notices in existing recording tests.

## NEEDS-SMOKE / acceptance remaining

- Claude: cargo test and clippy on macOS; Windows compilation/CI for the shared icon changes.
- Real microphone: early digital-zero warning during a long take, subsequent speech withdrawal, clean stop/paste, cancel, and an entirely digital-zero take that retains its finished-take warning. Test back-to-back takes and focus/cursor preservation.
- Native app icons: failed capture after a different app, failed icon lookup followed by retry, moved/reinstalled app bundle, and Insights real icons on macOS and Windows.
- Claude preview screenshots: all Settings panes at 800×560, About controls, dictionary header, island icons, and static preview at all six anchors. Browser preview was not attempted; no Chrome SIGABRT result is claimed.
- What's new: installed-version notes online, cached reopen, offline fallback/release-page opening, both navigation entry points and post-update “See what's new”. Verify the release workflow's generated release body and `latest.json.notes` on the next authorized release.

An unrelated untracked `PRODUCT.md` appeared during the session and was left untouched.
