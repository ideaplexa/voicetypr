# Plan 080 — UI completion (2.1.0-beta.4)

Status: SPEC — Claude 2026-09-29. It follows plan 079 (main window, pill and onboarding redesign, shipped in beta.3). The founder asked for the whole experience "from pill indicator to dashboard": every surface a user sees must match the 2.1 design language on macOS and Windows.

Process (founder cadence):
- One phase of 10 tasks.
- Each surface is designed in `design/voicetypr-2.1.pen` first, with its spec exported to `design/specs/` and a PNG to `design/exports/`.
- gpt-6.1-sol (medium) builds each task to spec.
- `pnpm ui:preview` screenshots are checked against the design.
- ONE gpt-6.1-sol (high) review at the end of the phase, one fix round verified by Claude, then the real-app E2E run, the PR and beta.4.

## Tasks

1. **Pill focus safety.** Make the pill provably non-activating:
   - macOS: a non-activating panel style mask, `canBecomeKeyWindow` NO (patch or replace the pinned tauri-nspanel behaviour).
   - Windows: `focusable(false)`, `WS_EX_NOACTIVATE`, SWP_NOACTIVATE on show.
   Proven recipe, from Handy's overlay (`voicetypr-archive/2026-09-29/handy-teardown/02-overlay-ux.md`):
   - macOS NSPanel: `can_become_key_window: false`, `is_floating_panel: true`, style mask `borderless().nonactivating_panel()`, `PanelLevel::Status`, `can_join_all_spaces` + `full_screen_auxiliary`, `no_activate(true)`.
   - Windows/Linux: `focusable(false)`, `focused(false)`, `skip_taskbar(true)`, `always_on_top`, `accept_first_mouse(true)` (the cancel button works on the first click), and re-assert `SetWindowPos(HWND_TOPMOST, …, SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_SHOWWINDOW)` after every show.
   - Also copy Handy's discipline: ONE reused overlay window, and deliver audio level only to the pill window label (`emit_to`).
   Then turn on the gated deferred terminal hide from plan 079 so "Pasted · N words" / "Copied" show in the default `when_recording` mode. Gate: E2E back-to-back and focus runs on macOS; Windows needs a CI or VM check.
2. **Pill redesign (alive + techy).**
   - Directions:
     - Round 1 (A Living Capsule, B Aura, C Morphing Island) was judged too safe.
     - Round 2 (D Segment dot-matrix, E Trace oscilloscope, F Halo tick ring) lives in `design/voicetypr-2.1.pen`, with live motion in `design/prototypes/pill-lab.html`, also embedded on the canvas.
     - The founder picks one direction or a mix, then build it.
   - Shared rules:
     - Honest level meter.
     - Level attack 30 ms / release 180 ms, peak-hold 300 ms.
     - Interruptible springs for every size change (k≈300, ζ≈0.78).
     - One accent with meaning: sage = done, amber = needs you.
     - No idle loops in ready/pasted/needs-you.
     - prefers-reduced-motion fallback.
     - Canvas at 2× DPR.
   - Performance and constraints:
     - Draw only while active.
     - Stays vanilla DOM + canvas (AGENTS.md invariant 8).
   - Research sources are cited in the lab page.
3. **Tray / menu-bar menu** (`design/exports/tray-menu-mac.png`, `tray-menu-win.png`). It stays a native Tauri menu (`menu/tray.rs`, `CheckMenuItem`/`Submenu`). The founder wants power users to be able to drive everything from it. Order:
   - **Status line** (disabled):
     - "● Ready · <engine> · on this Mac/PC"
     - amber + "Fix…" when the mic, model, license or permission blocks recording
   - **Start Dictation**, showing the user's effective shortcut.
   - **Quick settings:**
     - Polish ▸ Off / Clean / Writing / Notes / Message / Code / Per-app styles…
     - Engine ▸ downloaded local models, cloud engines, network servers, Download more models…
     - Microphone ▸ System default + devices
     - Mode ▸ Hold to talk / Press to start-stop
     - Live Preview ✓
     - Each shows its current value next to the label.
   - **History:** Copy Last Transcript, Recent ▸.
   - **App:** Open Voicetypr, Insights, Settings… (⌘, / Ctrl+,), Check for Updates…, Help & Feedback.
   - **Quit Voicetypr.**
   - Windows uses the same order, with Windows 11 menu metrics and leading icons.
   - Drop "Dashboard" and "Remote Voicetypr" headers. Network servers live inside Engine ▸.
   - The island peek's quick settings and this menu write the same settings through the same commands.
4. **Menu-bar icon + app icon.** Menu-bar template icon states (idle / recording / processing) and a refreshed app icon, designed in Pencil and exported to all the required sizes.
5. **Polish dialogs:** ProviderSetupDialog and AgentModelPickerDialog on the kit (provider cards, key field, model picker, test result).
6. **Transcription dialogs:** cloud ApiKeyModal (finish), OpenAICompatConfigModal, AddServerModal.
7. **Share stats:** ShareStatsModal and the share card render in the new style (sage accent, Geist).
8. **What's new + Crash report + Privacy consent dialogs** on the kit. "What's new" reads the real release notes.
9. **Toast window** (FeedbackToast, the native toast): the pill-family dark surface or a light card per theme, the same icons, and consistent copy.
10. **Windows visual pass.** Capture the real Windows app in CI (a screenshot job on windows-latest running the debug build with ui-preview fixtures, or a Windows VM). Fix title bar, scrollbar, font rendering, focus rings and Alt/Ctrl labels. Add the empty/error states that are still missing.

## Non-goals

- Functional changes. Those go to beta.5: Deepgram realtime default, Nova-3 multi, Polish upgrades, quantized Whisper, learned words, dictionary import, Windows CI smoke, Parakeet on Windows slice 5, 070b.

## Gates

- Frontend: typecheck, lint, vitest, build.
- Rust: cargo test, clippy, fmt (tasks 1, 3, 4, 9 touch Rust/native).
- `pnpm ui:preview` on both platforms and themes.
- E2E harness run before the PR.
- No transcript or content in logs/events.

## Product gaps found by the Island Lab scenario agents (2026-09-30)

The scenario agents read the real code. These are real behaviours, not prototype issues:

1. **A cloud failure loses the dictation.** When a cloud engine fails and `save_recordings` is off (the default), `finalize_in_flight_audio` deletes the temp WAV, so the words are gone. Keep the failed clip in temp until the user picks **Retry with <local engine>** or **Discard**. Offer that retry in the island through the existing `transcribe_audio_file` path. *(beta.4 island + backend)*
2. **Esc semantics.** The real app cancels on Esc twice within 2 s (`recording/escape_handler.rs`) and shows a "Press ESC again" toast in a separate window. The island should show that hint inline, since the stack is tucked while dictating. *(beta.4)*
3. **Dead listener.** `useAppEvents.ts` listens for `no-speech-detected`, but the backend never emits it. The real path is the pill toast from the speech-evidence gate. Wire one event, or remove the listener. *(beta.4)*
4. **One toast at a time.** FeedbackToast shows only the latest message. The island stack replaces this: newest in front, FIFO for timed items, sticky blockers, ×N merge, and timers paused while tucked. *(beta.4)*
5. **Toggle-mode copy.** "Too short — hold a bit longer" is wrong in toggle mode; use "talk a bit longer". *(beta.4)*
6. **Focus steal.** license-required calls `focus_main_window`. Show a license island state instead of stealing focus. Licenses are lifetime, so nothing expires and there is no "Renew". The real blockers are `RecordingLicenseState::VerificationRequired` / `CheckFailed` (`commands/audio.rs`): "Couldn't verify your license · Connect to the internet, then recheck" with **Recheck** (revalidate). *(beta.4)*
7. **Raw error strings.** For a missing mic, the pill flashes the raw `payload.error` string. Use the designed "No microphone · Choose mic" state. *(beta.4)*
8. **Timing mismatch.** The native terminal hide (pasted 1.2 s) must be aligned with the island's done card (3.2 s, with Undo/Original/Retry) once focus safety lands (task 1). *(beta.4)*
9. **Retry / Undo / Original need backend support.** There is no re-transcribe-and-replace-last-paste yet. Design "replace last paste" carefully: only do it if the target field still ends with our text, otherwise copy it. *(beta.5)*

## Founder feedback, 2026-10-02 (Island Lab review + using beta.3)

### Island v3 (supersedes the v2 listening layout; `design/prototypes/island-lab.html` is the motion spec)

- **The rest dot is not dead.**
  - At rest it breathes subtly. This must be a compositor-only CSS opacity animation on one element: no JS or canvas loop, and off under reduced motion. This amends task 2's "no idle loops" rule for the rest state only.
  - Hover opens a one-row *peek*:
    - left: a ready mark (sage), amber when the mic is missing
    - center: the shortcut hint with key caps
    - right: today's word count
  - Clicking the peek starts a dictation. The panel is non-activating, so the caret stays in the target app.
  - A mic problem turns the peek into "No microphone · Fix".
- **Three zones, the full width.**
  - Listening and live share one layout: app icon bottom-left, waveform in the center (it flexes to fill), timer bottom-right.
  - Live words span the full width above that row.
  - Rule for every state: left = identity, center = main content, right = meta or action. Fixed-width states never leave an empty side.
- **WhatsApp-style scrolling waveform.**
  - A new bar is born at the right about every 70 ms, sized to that slice's peak level. The strip slides left continuously, older bars dim, and silence shows dot-bars.
  - It replaces the 9 static bars.
  - On stop it still converges into the progress arc (one shared element).

### Main window

11. **Sidebar fixes** (`design/specs/sidebar.html`, `design/exports/sidebar.png`).
    - **Active item is white.** Today `data-active:bg-sidebar-accent` in `components/ui/sidebar.tsx` outranks our `bg-card`, so the selected item renders beige.
      - Fix it in `Sidebar.tsx` with `data-[active=true]:` overrides (`bg-card`, `text-foreground`, `font-semibold`). Never edit `ui/*`.
    - **Drop the "Setup" label.** A hairline divider separates Home / History / Insights from Transcription / Polish / Dictionary / Recording.
    - **Add Insights** after History.
12. **Insights page** (`design/specs/insights.html`, `design/exports/insights.png`). This is the gamified "what you've done" page.
    - Header: Week / Month / All time, plus **Share**.
    - Hero:
      - words dictated
      - time saved vs typing at 40 wpm
      - streak (current and best)
      - speaking pace in wpm (words ÷ `audio_duration_ms`)
      - dictations and average length
    - A 43-week activity calendar.
    - "Where you talk": top apps by words, from `writing.context_hint.app_name`. Only the app name, never window titles.
    - Milestones: word, streak and dictation thresholds and first Polish, plus a "Next" progress bar.
    - Data rules:
      - Extend `computeOverviewStats`. Every number is derived from local history, and no transcript text leaves the device.
      - Check first whether history is paged or pruned. If all-time totals can't come from the loaded rows, add one Rust aggregate command.
    - Home keeps its small weekly card and links to Insights.
13. **Share card** (replaces task 7's scope; `design/specs/share-card.html`, `design/exports/share-card.png`).
    - A 1200×630 dark card: total words, time saved, streak, pace, a 14-week mini calendar, voicetypr.com.
    - Rendered by `shareCardRenderer.ts`; ShareStatsModal previews it with Copy image / Save / Share to X.
    - Numbers only.
14. **Settings as a modal** (`design/specs/settings-modal.html`, `design/exports/settings-modal.png`). The founder's idea, Notion-style.
    - Settings opens over the current page from the sidebar's Settings item and ⌘, / Ctrl+,.
    - Left list: General, Shortcuts, Privacy, Storage | Advanced: Network sharing, CLI & API, Troubleshooting | Account: License, About & updates.
    - Esc or ✕ closes it.
    - Existing deep links (`resolveScreen` panes, the home status chip, the license chip) open the modal at that pane.
    - Below a 760 px window width it becomes a full-window sheet.
    - The Settings screen route goes away. Transcription, Polish, Dictionary and Recording stay as pages because people tune them often.

### Phasing (founder cadence: a phase of about 8–10 tasks, then one review)

- **Phase A (beta.4):**
  - 1 focus safety
  - 2 Island v3, with gaps 1–8
  - 9 toast → island stack
  - 3 tray
  - 11 sidebar
  - 12 Insights
  - 13 share card
  - 14 Settings modal
- **Phase B (beta.4, second review):**
  - 4 icons
  - 5 Polish dialogs
  - 6 transcription dialogs
  - 8 What's new / crash / privacy
  - 10 Windows pass
