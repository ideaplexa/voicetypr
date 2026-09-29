# Plan 079 — spec-match pass notes

The HTML exports in `design/specs/` are the reference, including Windows variants.
This pass changes frontend styling/layout and adds the separately requested manual
report ID. No Rust or `src/components/ui/*` edits; no commit.

## Source changes

- Main content starts at 40px, with 36px horizontal and 28px bottom padding.
  Removed AppShell's additional 36px margin and window inset. History retains its
  specified zero bottom padding. Page gaps: Home 22, Transcription 20, Recording 16,
  Polish/Dictionary/Settings/Help 18px.
- Sidebar width 212px; top padding 44px macOS / 16px Windows; navigation padding
  7px/10px, labels 13px, icons 16px, group label 10.5px/600 with 0.6px tracking.
  The transparent macOS top strip remains a Tauri drag region. The Windows toggle
  is at the top right so it cannot intercept the brand button.
- Shared card/row typography and spacing tightened. Cards use 14px radii,
  button wrapper uses 10px, segments use 7px within a 10px container.
  Switch wrapper uses the sage token in either theme and a white thumb.
- Home hero uses 26px/28px padding, 14px gaps and 30px headline typography;
  lower layout uses the 230px weekly card at the reference window size.
- Transcription options are compact horizontal cards; language selector no longer
  occupies a fixed 192px width. Progress/error states can wrap without overflow.
- Recording capture cards, 300px/120px pill preview and after-dictation cards
  tightened. Existing additional recording controls remain available.
- Polish uses the 300px per-app column, 18px cards and 19px example line height.
  Dictionary tabs and search share a toolbar; header/row spacing tightened.
- History, Settings, onboarding, Help and Windows exports were inspected; their
  existing screen-specific structures are retained, with shared shell/kit fixes.

## Deviations retained to preserve content and behavior

These are the identified source differences from the static reference states:

- Shell: sidebar collapse/toggle remains available (the static exports show an
  expanded sidebar without this control); responsive layouts and scroll behavior
  remain. Native traffic lights/titlebar and dragging need desktop smoke testing.
- Home: real rolling-seven-day statistics, their labels, dynamic readiness copy,
  individual accessible shortcut key caps, setup-chip icons, and history/loading/
  error states are retained. The reference uses illustrative weekly data and
  combined shortcut artwork; substituting those would change content.
- Transcription: the extra active-source status line, engine-dependent language/
  preview descriptions, download/verifying/error states, Whisper speed control,
  Soniox storage and Windows performance controls remain. Backend-provided model
  names are retained, including `Parakeet V3`. The frontend fallback mapping now
  says `Parakeet v3`; Rust supplies the uppercase label in
  `src-tauri/src/parakeet/models.rs` and remote discovery. No backend change made.
- Recording: Sounds retains its specific Recording started label/description,
  rather than replacing it with the reference's broader sounds description. The
  pill scale control, More card, detailed sound controls, media pause, shortcut
  editing/permission guidance and microphone state copy remain. These extra
  controls/content make the page taller than the static reference.
- Polish: provider configuration/error guidance, illustrative-example disclaimer,
  per-app editable fields/enabled switches/delete controls, and the More/final
  language card remain. The reference's simplified app rows omit those controls.
- Dictionary: existing Sounds like/Language/Status/Actions columns remain. The
  reference's Source/Used columns and Import control require data/behavior that
  this pass cannot add. Existing editor, search/empty states and explanatory copy
  remain; their content differs from the illustrative reference table.
- History: existing search/filter/sort/export/delete actions, upload flow, detail
  folder action and conditional status/error information remain beyond the static
  reference state. Their actual runtime behavior was not exercised here.
- Settings and onboarding: platform/permission/provider-dependent rows, loading/
  error states, update/license data and existing step logic remain. Static fixture
  labels are not substituted for live values. Windows native titlebar/permission
  behavior was not exercised on Windows hardware.
- Help: automatic-diagnostics disclosure and submission/error controls remain.
  The requested ID status and copy buttons intentionally add content to the spec:
  `Report sent · ID VT-XXXXX` on success, ID plus complete report fallback on failure.
- Shared controls: Base UI focus, disabled and selected-state affordances remain;
  no interactive primitive was changed. Additional/conditional controls absent
  from the exports retain their existing layout, with shared kit density updates.

## Verification

`pnpm ui:preview` could not open Chrome: the process exited with SIGABRT before
opening a page, producing zero screenshots. Consequently **every rendered screen
in both themes/platforms remains visually unverified**; no claim of measured ±1px
acceptance is made. Screenshots are needed to close that gate. No hardware smoke,
Windows artifact build, dictation E2E, server deployment or Discord delivery proof
is implied by the frontend tests.

Frontend test baseline: 932 tests. This pass adds four tests (three manual-ID
contracts and one sage-switch contract), retaining all existing tests: 936 total.

## Follow-up layout fix round

Compared the host screenshots in `.tmp/ui-preview/` with the HTML specs and PNG
exports for Transcription, Polish and Recording. Preserved the existing pass.

- ChoiceCard title/description now stack in both layouts; column descriptions
  use 12.5px normal-weight muted text below the 14px semibold title. Phase-one
  onboarding retains its horizontal card layout with stacked text.
- Each app-style rule uses one non-wrapping surface-2 row: flexible app input,
  visible style select, named enable switch, named delete button. The row keeps
  8px radius and 8px vertical / 10px horizontal padding.
- Removed download/verifying status and props from Spoken language; these states
  remain in the model list and do not disable the independent language selector.
- Sounds is now a compact summary. All three independent per-sound controls live
  in More, including Recording started; their setting updates are unchanged.

Added nine render/interaction regression tests. `pnpm typecheck && pnpm lint &&
pnpm test:frontend && pnpm build` passed: 83 files, 945 tests (previously 936).
The layout detector and `git diff --check` were clear. No Rust/UI primitives
changed; no commit was made.

Fresh visual verification remains pending: Chrome aborted with SIGABRT during
`pnpm ui:preview --only macos-light-transcription`; Aside CLI access failed, and
browser security review declined access to the local preview. Existing host
screenshots were inspected, but none were regenerated in this round.
