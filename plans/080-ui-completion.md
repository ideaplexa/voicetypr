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
3. **Tray / menu-bar menu.**
   - Relabel and regroup to the new IA: Open Voicetypr, then engine (current + switch), mode (Hold / Press), Polish on/off, microphone, then Copy last transcript, then Check for updates, Help & feedback, Quit.
   - Drop the "Dashboard" wording.
   - Same structure on Windows, where platform-appropriate.
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
6. **Focus steal.** license-required calls `focus_main_window`. Show a license island state with **Renew** instead of stealing focus. *(beta.4)*
7. **Raw error strings.** For a missing mic, the pill flashes the raw `payload.error` string. Use the designed "No microphone · Choose mic" state. *(beta.4)*
8. **Timing mismatch.** The native terminal hide (pasted 1.2 s) must be aligned with the island's done card (3.2 s, with Undo/Original/Retry) once focus safety lands (task 1). *(beta.4)*
9. **Retry / Undo / Original need backend support.** There is no re-transcribe-and-replace-last-paste yet. Design "replace last paste" carefully: only do it if the target field still ends with our text, otherwise copy it. *(beta.5)*
