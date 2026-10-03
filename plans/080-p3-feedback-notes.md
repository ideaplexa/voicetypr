# Plan 080 Phase A / P3 — island feedback

Implemented in `feat/2.1-beta4`; **uncommitted**. Source checks, native runtime acceptance and screenshot acceptance are separate.

## Stack and lifecycle

- `src/pill/feedback.ts`: exhaustive, content-free event copy and typed card commands.
- `src/pill/stack.ts`: newest-first display, three visible plus +N; arrival-order retirement, sticky blockers, ×N promotion/merge. Independent clips retain independent ids/actions; repeated events for the same clip merge.
- `src/pill/stack-view.ts`: shared lab springs, 19 px back-card peeks, .95/.90 back-card scales, 80 ms exits, reduced-motion opacity fallback, two hairlines. Existing island size initializes a handoff card while the island folds underneath; no intermediate dot-sized card. Top stacks open below the island.
- The renderer retains one demand-driven rAF. Timed clocks pause while dictating, processing and showing outcomes; clip leases expire on real wall time. Native pointer coordinates style stack buttons; the island and visible stack rectangles share the coalesced hit-region IPC.
- Copied/permission outcomes morph into sticky stack cards after 2.4 s, or tuck immediately when another take starts. A handoff does not announce the same outcome twice. Dismiss removes feedback without deleting clipboard text/audio; only Discard invokes deletion.
- Rust `pill_feedback` owns native visibility while any feedback remains, including exit animation. Its mount-ready queue covers initial blockers before the lazy pill subscribes. Kept expiry/eviction invalidates queued and visible recovery ids.
- `paste-outcome.polished` is a required bool, computed from the writing metadata's retained original and passed through insertion and clipboard-only delivery. No text is added to events. Pasted hover pauses its 2.4 s deadline; Original calls `copy_last_original`, then says “Original copied” only on a copied result. Undo/Retry have typed capabilities but no buttons.
- No main-window `island-navigate` consumer was added. Main-window Sonner toasts remain.

## Event → card/note table

Platform settings action: **Open System Settings** on macOS; **Open Privacy settings** on Windows.

| Event / kind | Copy | Action / lifetime |
| --- | --- | --- |
| recovery `cloud_failed` | `<engine> unreachable · Recording kept` | Retry with `<alt>` when available; Discard; real clip expiry |
| recovery `network_offline` | `Network offline · Recording kept` | Same |
| recovery `model_missing` | `<engine> isn't downloaded · Recording kept` | Same |
| recovery `remote_offline` | `<engine> is offline · Recording kept` | Same |
| recovery `integrity` | `Recording interrupted · Recording kept` | Same |
| recovery `no_speech` | `No speech heard`; `Audio kept for 30 s` | Transcribe anyway, ×; real 30 s lease |
| recovery `mic_dropped_empty` | `Mic disconnected · no audio captured` | Timed note |
| blocked `trial_ended` | `Trial ended`; `Activate Voicetypr to keep dictating` | Activate; sticky |
| blocked `license_check_failed`, `license_verify_required` | `Couldn't verify your license`; `Connect to the internet, then recheck` | Recheck; sticky |
| blocked `no_engine` | `No voice engine set up`; `Download a model or add a cloud key` | Set up; sticky |
| blocked `cloud_key_missing` | `Cloud key missing`; `Add a key to keep dictating` | Fix key; sticky |
| blocked `cloud_key_rejected` | `Cloud key rejected`; `Update the key in Settings` | Fix key; sticky |
| blocked `soniox_storage_full` | `Soniox storage is full`; `Delete old recordings to keep dictating` | Clean up; sticky |
| blocked `mic_permission_denied` | `Microphone access is off`; platform permission detail | Platform settings action; sticky |
| blocked `mic_missing` | `No microphone`; `Plug one in, or choose another` | Choose mic; sticky |
| blocked `mic_busy` | `Mic in use by another app`; `Close the call, or choose another mic` | Choose mic; sticky, red |
| blocked `accessibility_off` | `Allow Accessibility to paste automatically`; words copied / platform paste shortcut | Platform settings action; sticky |
| blocked `starting_up` | `Getting ready…` | Timed note |
| note `mic_dropped` | `Mic disconnected · using the 0:42 we got` (actual duration) | Timed |
| note `mic_silent` | `Your mic sent silence — is it muted?` | Choose mic; timed |
| note `translate_failed` | `Pasted untranslated · translation failed` | Timed |
| note `model_fallback` | `Using <B> · <A> isn't downloaded` | Timed |
| note `gpu_fallback` | `GPU unavailable · using the CPU` | Timed |
| note `polish_skipped` | `Pasted unpolished ·` + `Polish timed out` / `Polish rate limited` / `Polish unavailable offline` / `Polish kept the original` / `Polish failed` | Timed; no beta.5 replacement actions |
| `recording-too-short` | `Too short — hold/talk a bit longer`, using the event's mode | Timed, 2.2 s |
| `escape-hint` | `Press Esc again to cancel/discard` | Inline; 2 s; processing uses discard |
| `paste-outcome pasted` | `Pasted · N words` | 2.4 s; hover-only Original when polished |
| `paste-outcome copied` | `Copied — press ⌘V/Ctrl+V`; app didn't accept the paste | Sticky; × |
| `paste-outcome no_permission` | `Allow Accessibility to paste automatically`; words copied | Sticky; platform settings action, × |
| `island-notice finishing` | `Finishing…` | Inline during processing, 600 ms; no delayed stale stack notice |
| other `island-notice` kinds | Polish on; · not polished; Polish setup; shortcuts retired; still listening; auto-stop with captured/no audio distinction; recording/copy/transcription failure; History retry; no speech; Hold on… | Enumerated Rust/TS kinds; no free text |

## Toast call-site conversion inventory

The locations below are **pre-change HEAD line numbers** so removed calls remain auditable. All wrappers/types/counters and the floating `toast` event were deleted. Runtime source has no remaining retired references; the retirement test intentionally names the deleted artifacts.

| Previous site | Replacement |
| --- | --- |
| `commands/audio.rs:1482` | Existing P4 `recording-too-short`; removed duplicate toast |
| `commands/audio.rs:2360` | P4 `polish_skipped` note in the shared manual/file Polish failure path |
| `commands/audio.rs:5034` | Enumerated `island-notice-clear` for active silence notices |
| `commands/audio.rs:5065` | P4 `mic_silent` note / Choose mic |
| `commands/audio.rs:5073` | `long_silence` notice |
| `commands/audio.rs:5085` | `silence_stopped` notice |
| `commands/audio.rs:5110` | `silence_discarded` notice |
| `commands/audio.rs:5120` | `recording_failed` notice |
| `commands/audio.rs:5644` | `recording_failed` notice for an unclassified failed initialization |
| `commands/audio.rs:5705`, `5783` | Existing P4 mic blocker; removed duplicate free-text classifier/toast |
| `commands/audio.rs:6246`, `6307` | Existing retained `dictation-recovery`; removed duplicate toast |
| `commands/audio.rs:7193` | `no_speech` notice for an engine returning no usable text; makes no retained-audio promise |
| `commands/audio.rs:7266`, `7342`, `7359` | Existing P4 Polish/translation fallback note; removed duplicate toast |
| `commands/audio.rs:7524`, `7533`, `7583` | `paste-outcome` copied/permission card; removed duplicate toast and coalesced permission blocker |
| `commands/audio.rs:7592` | `copy_failed` notice |
| `commands/audio.rs:7755` | `history_retry` or `transcription_failed`, based on durable history availability |
| `commands/audio.rs:7822` | `history_retry` notice |
| `commands/audio.rs:7830`, `7839`, `7848` | `transcription_failed` notice in the residual non-recovery error path |
| `commands/text.rs:507`, `525` | Caller emits the real copied outcome; removed duplicate toast and clipboard-layer app handle |
| `commands/shortcuts.rs:650`, `664`, `668`, `849` | `polish_on`, `polish_off`, `polish_setup`, `shortcuts_retired` |
| `transcription/engines.rs:160` | Existing P4 `gpu_fallback`; removed duplicate toast |
| `recording/escape_handler.rs:64` | Existing P4 `escape-hint`; removed duplicate toast and second-Esc toast hide |
| `recording/hotkeys.rs:59` | `shortcut_throttled` notice; processing presses additionally show inline `finishing` |

Deleted files: `src/toast.tsx`, `toast.html`, `src/components/FeedbackToast.tsx`, `src/components/FeedbackToast.test.tsx`. Removed native window construction/configuration, hide command registry, Vite input, all three capability window labels and separate positioning/tests. Updated architecture map and obsolete routing comments.

## Validation and outstanding gates

- Typecheck and oxlint passed.
- Full Vitest: **1,155 tests across 98 files**, versus 1,073 before this slice (8 toast tests removed, 90 island/retirement cases added; net +82). The full run includes **171 pill/stack cases**; the preceding dedicated run passed 170 before the permission-coalescing case was added.
- Production build passed; existing main bundle chunk-size warning remains.
- Rust format check passed. Replaced 12 removed legacy Rust toast/position tests with 14 enumerated notice contracts and a polished provenance test; source test count does not drop. These Rust tests have **not executed**.
- `cargo test` and `cargo clippy --workspace --all-targets -- -D warnings` both stop in `build.rs` before checking this Rust code: SwiftPM's nested `sandbox-exec` reports `sandbox_apply: Operation not permitted`. No sidecar/build-script bypass was made.
- `pnpm ui:preview --only pill`: installed Chrome **SIGABRTed before opening a page**, zero screenshots. Claude must capture macOS/Windows, light/dark, stack3/+N, every blocked/recovery/note kind, pasted-original hover, finishing and morph-end, then compare the v32 reference shots. Preview states are listed in `scripts/ui-preview/shoot.mjs`; both platforms use the same entry.
- Logs: `.tmp/p3-{typecheck,lint,vitest,targeted,build,cargo-test,clippy,fmt,preview}.log`.

**NEEDS-SMOKE:** native no-focus-steal/paste behavior, pointer hit testing over every visible card and empty canvas, all six anchors (especially top), Windows work area/DPI, real-speech retry/transcribe/discard and 30 s no-speech lease, card retention through new takes, native visibility through animated exit, real clipboard Original and no-permission settings actions, back-to-back dictation. Source checks and browser fixtures do not prove these.
