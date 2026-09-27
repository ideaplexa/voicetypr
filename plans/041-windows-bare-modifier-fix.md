# Plan 041 — Fix: Windows bare-modifier hotkey dead during onboarding test step

> User-reported, investigation-confirmed (2026-07-02 Explore, file:line evidence).
> Symptom: on Windows, a lone Ctrl/Alt hotkey does nothing during onboarding's
> first_transcription step. Compound cause: the in-app DOM fallback
> (src/hooks/useInAppRecordingHotkey.ts — exists because WebView2 can starve the
> LL-hook path when our own window has focus, :82-90) has coverage gaps, plus one
> latent keytrigger gap. Fix ALL FOUR; each is independently real.

## Fixes

### F1 — Fallback must support modifier_hold (hold-to-talk)
useInAppRecordingHotkey.ts:71-77 `tapToggleModifier` returns null unless
action==="toggle_recording" && trigger_kind==="isolated_tap" (test :310 locks this
by design — change the design). Add hold support: on keydown of the bound bare
modifier in an editable target → start (hold_to_record semantics: start recording),
on keyup → stop. Reuse whatever command surface the tap path uses (verify how tap
invokes toggle — likely invoke("toggle_recording") or dispatch equivalent; for hold
use the matching start/stop or hold action the engine path uses,
recording/hotkeys.rs dispatch_action is the reference for what modifier_hold
triggers natively). Keep repeat-keydown suppression (Windows auto-repeat fires
repeated WM_KEYDOWN; only the first arms the hold).

### F2 — Onboarding: focus the sample textarea
OnboardingDesktop.tsx first_transcription step (~:1403): the sample Textarea is not
auto-focused, so the editable-focus-gated fallback never fires after step entry.
Auto-focus the textarea when the step becomes active (and re-focus after the user
clicks the step's buttons). Do NOT remove the editable-focus gate globally — it
prevents double-firing with the native path outside our windows; focusing the
textarea makes the gate hold exactly where the user tests.

### F3 — Fix the fallback arming race
useInAppRecordingHotkey.ts:121-144 arms only when useSetting("hotkey") is already
"" — during onboarding, hotkey transitions to "" across three async invokes
(OnboardingDesktop.tsx:616-659). Rearm deterministically: re-run reloadBareModifier
when shortcut settings change (listen to the same event/store change the engine
rebuild uses — verify what frontend-visible signal exists, e.g. settings-changed
event or the update_shortcut_settings resolution) instead of only the cached
useSetting value; ensure the final state after onboarding save always arms.

### F4 — keytrigger Windows: normalize generic modifier VKs
crates/keytrigger src/backend/windows.rs:198-207 map_vk maps only side-specific
VKs (0xA0-0xA5); generic VK_SHIFT/VK_CONTROL/VK_MENU (0x10/0x11/0x12) fall through
as KeySpec::Raw with side=None (acknowledged vk.rs:44-45) → matcher never sees a
modifier. Map generic VKs to a side using the KBDLLHOOKSTRUCT scan code / extended
flag (LLKHF_EXTENDED distinguishes right Ctrl/Alt; scancode 0x36 = right shift),
defaulting to Left when indeterminate. Unit-test the mapping function with
synthetic KBDLLHOOKSTRUCT-shaped inputs (pure function tests compile+run on macOS
if the mapping is extracted into a cfg-independent helper — do that; the hook
itself stays cfg(windows)).

## Tests
- TS: extend useInAppRecordingHotkey.test.tsx — modifier_hold keydown/keyup starts/
  stops (replacing/updating the :310 "ignores modifier_hold" test), auto-repeat
  keydown suppressed, arming after shortcut-settings change event (F3), existing
  tap tests keep passing. OnboardingDesktop.test.tsx: textarea focused on step
  entry (F2).
- Rust: pure mapping tests for F4 (runnable on macOS); existing keytrigger matcher
  tests untouched.

## Acceptance
- pnpm typecheck/lint/test pass; cargo test passes (keytrigger crate included).
- Honest limitation stated in your report: end-to-end Windows verification (real
  lone-Ctrl press on a Windows machine in onboarding) CANNOT be done here — list
  exactly what the user must manually verify on Windows (tap mode, hold mode,
  outside-app usage unaffected, no double-fire when clicking in other apps'
  fields).
- Do not commit.

## Fallback decision (user rule)
If Windows manual verification later fails, the "test your first dictation" step
gets removed per the user's rule (bad test worse than no test). NOT in this slice.
