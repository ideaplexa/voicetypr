# Plan 053 — Reset-on-prop & parent-notify effect elimination

Baseline: react-doctor 55/100 (Critical). Plan 052 cleared the store/derive
targets; this plan clears the remaining state-sync effects react-doctor flags:
"All state reset on prop change", "State adjusted after a prop changes ×19",
"Data passed to parent via effect", "Parent kept in sync with a callback
effect ×4", "Effects chained together".

## Doctrine

Per React "You Might Not Need an Effect":
- Reset on prop change → adjust state during render (prev-value compare), or
  reset in the event handler that flips the prop. Prefer render-adjust when
  Base UI exit animations must survive (no key-remount of open dialogs).
- Notify parent → call the callback from the event handler, not an effect.
- Derived value → compute it; never mirror into state via effect.

## Targets

1. `ApiKeyModal` — clear input on close (effect → render-adjust).
2. `OpenAICompatConfigModal` — sync defaults when opened (effect →
   render-adjust); `setTestResult(null)` on input change (effect → derived
   staleness against last-tested snapshot).
3. `AddServerModal` — sync form from `editServer` when opened (effect →
   render-adjust).
4. `ShareStatsModal` — reset state when closed (effect → render-adjust).
5. `HotkeyInput` — live pendingHotkey/pendingBareModifier pushed to parent via
   effects (→ invoke callbacks from the capture handlers).
6. `MicrophoneSelection` — parent synced via callback effect (→ handler).
7. `RemoteServerCard` — `setControl(null)` on status change (→ derive or
   render-adjust).
8. `RecordingSettings` — `setNativeBinding(null)` when hotkey changes
   (→ render-adjust).

Items 6–8 were reviewed during execution and kept as legitimate effects
(see below).

## Reviewed and kept (legit effects)

- `RemoteServerCard:170` — conditional async fetch keyed on server status.
- `RecordingSettings:170` — async binding fetch keyed on `settings.hotkey`.
- `MicrophoneSelection:86` — corrective action when the OS device list
  changes (external system); folding it into the fetch handlers would need
  `value`-ref plumbing that obscures intent.

## Outcome

react-doctor score 55 → 62 (Critical → Needs work); targeted rules no longer
fire on ApiKeyModal, OpenAICompatConfigModal, AddServerModal, ShareStatsModal,
HotkeyInput. 663/663 frontend tests; Shortcuts tab smoke post-change.

## Non-goals

- The 14 "large component" findings (separate maintainability pass).
- Converting the remaining legitimate subscriptions (052 doctrine covers).

## Acceptance

- `pnpm typecheck`, `pnpm lint`, full `pnpm vitest run` green.
- react-doctor: the targeted rules no longer fire on these files.
- Local macOS smoke of Polish (custom config modal), Shortcuts (hotkey
  capture), Sources (microphone selection) surfaces unchanged.
