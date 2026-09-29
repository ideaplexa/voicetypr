# S8 implementation and acceptance notes

The source, readiness, permissions, hotkey and success handlers remain in
`useOnboardingDesktop`. Welcome advances into the phase-one header. The macOS
step order moves readiness before permissions to make the requested visual
phases sequential; step identifiers, settings keys, consent writes, completion
writes and the existing request/validation/setup handlers remain. There is no
persisted step-progress key or step-change telemetry call in this hook to rename.
Windows uses its existing hotkey step to host the microphone phase, then saves the
shortcut once before the trial. Changing the shortcut opens the existing editor
inside phase three while retaining trial text. The trial reuses Home's
`useTestDictation`, without a second delivery-success implementation.

## Deviations from the four HTML specs

- Model names and download sizes come from the platform's model catalog, rather
  than the illustrated Parakeet/500 MB string. Sizes follow the existing model UI's
  byte-to-MB conversion. A ready model's action says Continue; cloud and remote
  actions describe connecting the chosen source.
- Source setup expands into the existing readiness controls in the same 620 px
  phase frame. Local selection, download/progress/cancel/repair/delete, Windows
  GPU settings, cloud key validation and remote discovery/setup require controls
  absent from the static source-choice illustration. API-key entry uses the
  existing key form inline; remote server/password setup retains its existing modal.
- Phase two omits the illustrated meter and says “You'll test it in the next
  step.” There is no pre-recording level source: Rust emits recording levels only
  to the pill window. The mic card says Microphone instead of Say something;
  its subtitle asks users to choose a microphone rather than implying it is
  already listening. The unconditional “nothing leaves” claim is omitted because
  remote transcription can send audio to another computer. The macOS-only explanatory Windows footnote is omitted.
- The microphone selector uses real settings and `set_audio_device`, but named
  devices cannot be enumerated during a fresh onboarding: `get_audio_devices`
  deliberately returns an empty list until `onboarding_completed` is true.
  Default is available. No completion flag is changed to bypass this gate, and no
  Rust change is made. Preview device names are fixtures, not hardware evidence.
- Permission status and actions are live rather than the illustrated fixed
  Allowed/denied pair. Allowed is also a recheck control. Both denied permissions
  use the existing request flow with Open System Settings as its label.
- Text below 14 px uses muted-foreground, including recommendation tags, statuses,
  secondary buttons, mic values and Worked. Primary action labels are 14 px
  instead of 13 px to preserve the specified primary fill and readable text while
  respecting the small-text requirement.
- KeyCaps use Geist, the S1 font, instead of the export's Inter. Key labels and
  hold/press instructions come from the real configured shortcut and recording
  mode. The trial starts empty and reports Worked · N words only after a pasted
  outcome plus an input change; it never asserts the illustrated 0.4 s latency.
- There is no static “Pasted · 9 words” pill inside the main window. The real native
  pill owns recording and delivery state, preventing a fabricated success display.
- The two existing privacy choices remain below the phase-three actions. Their
  consent controls and completion failure/retry behavior must remain reachable;
  they are absent from the illustration and add height to the phase-three content.
  The existing shortcut editor is an additional inline state absent from the specs.
- The layout fits the native window and can scroll on shorter viewports instead
  of fixing a 1000 × 680 canvas or adding a second decorative outer window frame.
  Dark mode maps all illustrated light hex colors to the S1 semantic tokens.

## Verification boundaries

Required source gates: typecheck, oxlint, frontend suite and production build.
Onboarding tests: 23 before, 29 after; existing scenarios retained.

`ui-preview.html?onboarding=1|2|3&platform=macos|windows&theme=light|dark`
uses the real AppContainer onboarding gate with explicit preview fixtures.
`shoot.mjs --only onboarding` includes all twelve combinations.

Screenshot capture is blocked in this environment: headless Chrome exits with
SIGABRT before opening a page, Aside is unavailable, and automatic browser
approval rejected access to the local preview URL because permission was denied.
Therefore ±1 px visual matching has not been verified by screenshots.

Hardware dictation remains NEEDS-SMOKE. The E2E preflight reports missing BlackHole
input/output devices and unavailable TextEdit Automation. No real dictation,
Windows CI artifact or native desktop visual acceptance is claimed.
