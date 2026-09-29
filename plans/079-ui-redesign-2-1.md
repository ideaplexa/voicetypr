# Plan 079 — Voicetypr 2.1 UI redesign (main window, pill, onboarding)

Status: SPEC — Claude 2026-09-29. Branch `feat/2.1-ui` (from `feat/2.1-beta2` @ 18b46f33).
Design source: `design/voicetypr-2.1.pen` (+ PNGs in `design/exports/`). Target: 2.1.0-beta.3/4.

## Why

The founder: "our UI/UX is too ugly — the whole interaction experience." Inventory (2026-09-29):
13 nav items, 3 page-header patterns, 5+ card patterns, 3 setting-row patterns, 4 page-width
wrappers, a pill with hardcoded colors and no dark variant, Dictionary/Corrections/Snippets
buried inside Polish, Upload as its own screen. The redesign is **structural consistency first,
pixels second**: one header, one card, one row, one IA.

## Target IA (sidebar, 8 items + license chip)

| Sidebar | Screen id | Built from (current) |
|---|---|---|
| Home | `home` (was `overview`) | OverviewTab: CurrentSetupCard, WeeklyRhythmCard, ShareStatsModal |
| History | `history` (was `recordings`) | RecentRecordings* **+ Upload** (`AudioUploadSection` → "Transcribe a file…" sheet/dialog) |
| SETUP · Transcription | `transcription` (was `models`) | ModelsSection (local, cloud, remote), language, live preview, SonioxStorageCard, TranscriptionPerformanceCard (Windows) |
| SETUP · Polish | `polish` (was `formatting`) | EnhancementSettings minus the word editors: provider, default style, per-app styles, keep-my-words / skip toggles |
| SETUP · Dictionary | `dictionary` (new) | CustomWordEditor (Words), ReplacementEditor (Corrections), SnippetEditor (Snippets) as tabs |
| SETUP · Recording | `recording` | CaptureControls (shortcut + mic), pill options, after-dictation (paste / clipboard / sounds) |
| Settings | `settings` | panes: General (Appearance, Open at login, Updates, Update channel, Menu bar, **About: version + Check for updates**), Shortcuts, Privacy (TelemetrySection), Storage (StorageCleanupCard); ADVANCED: Network sharing, CLI & API, Troubleshooting (AdvancedSection: permissions, quick fixes, Reset) |
| Help & feedback | `help` (was `report-problem`) | ReportProblemSection (+ links; the Discord link arrives later) |
| License chip | `license` (not in nav list) | AccountSection; chip shows plan + version, click opens License |

Old ids stay as aliases in `TabContainer` for one release so the `navigate-to-overview`,
`license-required` and `soniox-storage-limit` events and deep links keep working.

## Design tokens (Tailwind v4, `src/globals.css`)

- Keep the shadcn semantic names. `--accent` stays the neutral hover grey. Do NOT repoint it
  (that would recolour every `hover:bg-accent`).
- Brand colour = the existing `--sage` family, retuned: sage `#3F7D5C` light / `#8FD1A8` dark;
  `--sage-bg` becomes accent-soft `#E3EFE7` / `#1F3328`.
- Retune values to the .pen tokens:
  - background `#F7F7F5`/`#141415`, card `#FFFFFF`/`#1C1C1E`, muted (surface-2) `#F1F1EE`/`#242426`
  - sidebar `#EFEFEC`/`#19191A`, border `#E4E4E0`/`#2C2C2F`
  - foreground `#18181A`/`#F2F2F0`, muted-foreground (text-2) `#6A6A66`/`#A3A3A0`
  - add `--text-3` `#9A9A95`/`#6E6E6B`, `--warn` `#B26A00`/`#F2B35B`, `--warn-bg`
  - destructive `#C2412D`/`#F28B7A`
- Radius: card 14px, control 10px, segment 7px.
- Font: Geist Variable (already installed). Add `@fontsource-variable/geist-mono` for times and
  counts, or drop `--font-mono` to `ui-monospace`. Pick one; no undeclared fonts.
- Hardcoded colours to tokenize: the Sidebar license badge, EnhancementSettings' amber notice,
  and all of `pill.css`.
- Fix `ui/sonner.tsx`: pass the resolved theme from `useTheme` instead of `next-themes`
  (there is no provider, so toasts are always "system").

## One kit (`src/components/settings/settings-ui.tsx`, extended, not a second kit)

- `SettingsPage`: one width (`max-w-3xl`, centred) with a `wide` variant (History, Dictionary).
- `PageHeader`: title 24/600, one-line description, optional right-side action. It replaces
  all three header patterns, including the "?" guide dialogs, which move to an `InfoButton`
  in the action slot.
- `SettingsCard`: 14 radius, 1px border, `bg-card`. FieldSet cards, rounded-2xl icon-chip
  cards and `border/50` cards all migrate to it.
- `SettingRow`: title 13.5/500 + description 12 on the left, control on the right, bottom
  divider inside cards.
- `ChoiceCard` (engine/option cards with a selected ring) and `Segmented` (Clean / Writing /
  Message / Code; System / Light / Dark) extend shadcn primitives. `ui/*` is not modified.

## Slices (each slice is one Sol commit series, reviewed by Astra, gated, screenshotted)

- **S0 — prerequisites (no visual change).**
  - Hoist the listeners that only run while a tab is mounted into an app-level hook
    (`useAppEvents`), so moving tabs cannot drop them:
    - SettingsTab: `hotkey-registration-failed`, `no-speech-detected`
    - EnhancementsTab: AI enhancement errors
    - AccountTab: `license-required`
  - Extract a shared `useWritingSettings` store (one save queue for words, corrections,
    snippets and `app_formatting_rules`) so that Polish and Dictionary can both edit without
    racing. Today `usePolishSectionSettings.ts:28-93` owns it.
  - Tests: listeners fire with no tab mounted; concurrent edits from two consumers keep one
    ordered save queue.
- **S1 — tokens, kit, sidebar and IA.** Token retune, Geist Mono decision, kit components,
  new `navigation.ts` (8 items + SETUP group + footer), license chip, old-id aliases, and the
  About row (version + update check moved out of the sidebar). Update `Sidebar.test`,
  `TabContainer.test`, `AppShell.test` and `AppContainer.test` to assert the new
  **user-visible** labels and order, not class names.
- **S2 — Home.** Hero ("Press ⌥ Space and start talking" using the real formatted shortcut,
  hold/toggle copy from settings), status badge (engine ready / downloading / needs key), setup
  chips (engine, language, polish, live preview; each clickable to its screen), Recent (4 rows,
  "View all history →"), This week card (existing stats hook). "Try a test dictation" opens the
  in-app try box (it reuses the onboarding success step component).
- **S3 — Transcription + Recording.**
  - Transcription:
    - three ChoiceCards (On this computer / Cloud / Another computer)
    - one model list with radio + state (In use / Downloaded / Download / progress)
    - Spoken language + Live preview rows; Soniox storage + Windows performance underneath
    - Windows copy says "this PC"
  - Recording: shortcut capture + hold/toggle segmented control, mic select + live level
    meter, pill options with a live preview, after-dictation rows.
- **S4 — Polish / Dictionary split.**
  - Polish: master switch, default style segmented + before/after example, provider card,
    keep-my-words / skip-when-clean rows, per-app styles.
  - Dictionary: Words / Corrections / Snippets tabs, search, table with source tag
    (You / Imported / Learned when 080 lands), used count, Add; import stays.
  - Update the `EnhancementsSection.test` region assertions.
- **S5 — History + Upload merge.**
  - Split view: list with day headers + app + time; detail with text, "Before polish"
    original, chips, Copy / Re-transcribe / Delete.
  - "Transcribe a file…" opens the upload flow in a dialog; results land in History.
  - Remove the Upload screen. Copy that names "Sources" or "Quick help" is updated.
- **S6 — Settings with Advanced.** Inner pane nav (General, Shortcuts, Privacy, Storage ·
  ADVANCED: Network sharing, CLI & API, Troubleshooting). Remove the duplicate h2 titles
  (AgentCliSection:141, TelemetrySection:100).
- **S7 — Pill.**
  - Retokenize `pill.css`: one dark surface `#121316`, a hairline border, sage bars, Geist.
    The pill stays dark in both themes, by design.
  - New terminal states from the existing Rust `PasteOutcome` (`commands/text.rs:172`):
    - `Pasted` → "Pasted · N words" (✓)
    - `LeftInClipboard` / `NoPermission` → "Copied — press ⌘V" (Ctrl+V on Windows),
      shown for 1.6 s
  - This needs one new event (`paste-outcome` with `{outcome, words}`, no text). The existing
    states map to Listening / Transcribing / Polishing / Error / Too short.
  - "No text field" detection (AX focused-element role on macOS, UIA on Windows) is **out of
    scope**: it's a later plan, and the copy must not claim it.
- **S8 — Onboarding.** Three steps (Choose engine → permissions + mic check, macOS only for
  permissions → Try it now), same kit, progress pill. Keep the current step logic in
  `useOnboardingDesktop`; only the chrome and copy change. The engine step recommends
  on-this-computer and shows the real model size.
- **S9 — Help & feedback + polish pass.** Report a problem (report ID; the Discord link comes
  later, and the email ask is removed per UI direction), empty states for History, Dictionary
  and the model list, focus rings, keyboard nav through the sidebar, and a dark-mode sweep of
  every screen.

## Gates (every slice)

1. `pnpm typecheck && pnpm lint && pnpm test:frontend` green; `pnpm check` before the
   slice merges into `feat/2.1-ui`.
2. Astra (`gpt-6-astra --effort medium`) adversarial review; findings fixed or rebutted in
   writing.
3. Visual check in light + dark:
   - macOS: debug build screenshot of the changed screens via the Cap CLI, compared to the
     .pen export.
   - Windows: the CI `build-windows` artifact must build. The Windows-specific checks are key
     labels (Alt/Ctrl/Win), "this PC" copy, the native titlebar and no traffic-light padding
     (`AppShell.tsx:73`).
4. The E2E dictation harness (`scripts/e2e`) passes after S1, S3, S7 and S8 (nav, pill and
   onboarding touch the recording path).
5. No transcript text in logs or events (the S7 `paste-outcome` carries counts only).

## Non-goals / STOP conditions

- No backend behaviour changes except the S7 `paste-outcome` event and S0 listener hoisting.
  STOP and ask if a slice needs more.
- Do not modify `src/components/ui/*` shadcn primitives; extend or wrap them.
- Do not change settings keys or the persisted settings schema. Screen ids change; stored
  settings do not.
- No new heavy dependencies (the Geist Mono fontsource package is the only allowed add).
- Brand spelling "Voicetypr" everywhere.
