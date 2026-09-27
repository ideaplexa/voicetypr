# 033 — Tray recovery and upload result accessibility

Status: DONE (code) — NEEDS-SMOKE on signed `v2.0.5-beta.7`.

## Problem

Two 2.0.4 reports expose release-blocking usability gaps:

- System-tray construction is nonfatal after PR #96, but the app gives up after five startup attempts, records no first-attempt success, exposes no runtime status to support, and offers no recovery without restarting. A macOS report therefore cannot distinguish creation failure from an OS-hidden status item.
- A long Parakeet upload can return enough speaker segments for the timeline to escape its `max-h-48` ScrollArea and cover the Copy/Save controls.

The 2.0.4 recording-start/end sounds are intentional and remain controlled by existing settings; this plan does not change them.

## Change

Keep both release fixes in one PR while preserving separate implementation seams.

### Tray

- Extract tray construction from the setup closure into a reusable helper without changing menu actions.
- Preserve five immediate startup attempts and the no-crash/no-headless guarantees from PR #96.
- Track `Available` or `Unavailable { attempts, last_error }` in managed runtime state.
- Log every creation outcome, including first-attempt success.
- After immediate attempts fail, keep the main window visible and run bounded delayed recovery attempts. A recovery must never hide the main window or create a duplicate tray.
- Expose a Tauri command for tray diagnostics and a manual retry action from the visible dashboard.
- Include tray availability in bug-report payloads and correct logs that claim menubar mode when hiding was skipped.

### Upload

- Give the speaker timeline a definite scroll height.
- Keep Copy, Save, and Transcribe Another File outside the scrolling viewport and reachable for long segment lists.

## Non-goals

- Do not change tray artwork, macOS menu-bar permissions, update channels, or audio-feedback defaults.
- Do not treat notch/menu-bar crowding as a tray-construction failure.
- Do not add unbounded background retries or hide the dashboard after delayed recovery.

## Automated acceptance

- Pure retry/status tests cover immediate success, transient failure then recovery, persistent failure, manual retry, and duplicate prevention.
- Existing close/hide behavior remains guarded when the tray is unavailable.
- Tray diagnostics command is registered and returns sanitized status only.
- Upload component tests render a large speaker timeline with all result actions present.
- Frontend typecheck/lint/tests and backend fmt/clippy/check/tests pass.

## Runtime smoke

Release in `v2.0.5-beta.7`, then verify:

- macOS signed build: fresh launch, quit/relaunch, sleep/wake, crowded notched menu bar, external display, and manual tray retry without duplicate icons.
- Windows signed build: normal launch/autostart, tray actions, and the existing Plan 030 device/display matrix.
- Long Parakeet upload: timeline scrolls internally; Copy and Save remain mouse- and keyboard-accessible; saved transcript matches History.
