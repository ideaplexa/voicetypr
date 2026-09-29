# S9 — Help & feedback and License

Sources: `design/specs/help.html`, `design/exports/help.png`, and the S1 kit in
`src/components/settings/settings-ui.tsx`. Changes are uncommitted.

## Existing report contract

Manual reports POST to the existing `VITE_BUG_REPORT_ENDPOINT` (default
`https://voicetypr.com/api/v1/bug-reports`), through `submitManualReport`.
There is no Rust submission command. The existing diagnostics commands
(`get_latest_log_for_bug_report`, `get_system_specs`, `get_device_id`, and tray
status) are unchanged. Environment and latestLog payloads are unchanged.
Name/email are optional in the existing frontend endpoint payload; Help no
longer collects them, and undefined contact properties are omitted from both
gathered data and the constructed payload. No empty contact values are sent.

The existing `ReportSubmitResult` and response parser expose success/message,
not a report ID. Success therefore keeps “Report submitted. Thank you.” in the
toast and adds that same copy to the screen. No ID or optional-logs contract was
invented. Failed submission still shows “Report not sent” and the prepared-report
clipboard fallback; collection failure retains its existing toast and message.

## Token mapping

| Export hex | Token |
| --- | --- |
| #F7F7F5 | background (page, textarea, primary-button text) |
| #FFFFFF | card |
| #F1F1EE | muted (tile icon surfaces) |
| #18181A | foreground / primary |
| #6A6A66 | muted-foreground |
| #E4E4E0 | border |
| #3F7D5C | sage (success icon); ring (focus) |
| #9A9A95 | replaced with muted-foreground for the 13px placeholder |

Dark mode resolves these same semantic tokens to the S1 dark palette. License
also uses sage-bg, warn, warn-bg, and muted rather than the previous green/amber
utility palettes. All text under 14px on these screens uses muted-foreground.

## Known spec deviations and reasons

1. **Diagnostics:** fixed disclosure replaces the depicted 34×20 switch. The
   existing flow always attaches diagnostics and has no optional-log contract.
   Copy lists the actual attachments, including device ID, instead of implying
   that the logs contain only settings/errors. The longer disclosure can wrap,
   increasing this row and the report card height.
2. **Success:** existing success copy replaces the depicted “Last report sent ·
   ID VT-8K2F9”. It appears only after successful submission, rather than being
   prefilled. No report ID is exposed by the existing flow, so there is no mono
   ID. Before submission that slot is empty.
3. **Version:** the tile uses Tauri `getVersion()` instead of hardcoding
   `2.1.0-beta.3`. Until version loading succeeds it has neutral copy and is
   disabled. Its action opens the existing `UpdateAnnouncementDialog`.
4. **Small-text colors:** tile Open/Read text changes from sage to
   muted-foreground; textarea placeholder changes from text-3 to
   muted-foreground; textarea input, disclosure, validation, and success copy
   also use muted-foreground. This follows the founder's <14px rule.
5. **Send button:** text is 14px instead of 13px so the primary button can keep
   readable light text on its dark surface. Padding remains 9px×16px, radius
   10px, and text maps to background as in the export. The extra 1px in type may
   change button width and height.
6. **Card edges:** a 1px inset token ring replaces the export's 1px outline at
   -0.5px. This keeps the export's padding inside the box without adding a
   layout border; edge placement differs by at most 0.5px.
7. **App shell:** existing S1 sidebar/native titlebar are retained. The shell
   uses a 224px sidebar, 36px titlebar, 8px right/bottom inset, and rounded main
   inset, unlike the export's approximately 212px sidebar and standalone
   1000×680 window. Help compensates with local top/left/right padding of
   4/24/28px rather than 40/36/36px, targeting the same page origin and right
   edge at 1000×680. Sidebar spacing, branding/footer typography, shell corners,
   titlebar controls, and native Windows chrome remain the existing S1 output.
   Their whole-window differences are outside these screen edits.
8. **Responsive layout:** below the small breakpoint tiles stack instead of
   forcing the export's fixed three-column 1000px composition. Text can wrap;
   pages scroll to preserve access at smaller window sizes.
9. **Interactive states:** textarea remains vertically resizable, max 5000
   characters, and has validation/loading/disabled states. Errors and Copy
   report extend the depicted success-only card when needed. Buttons retain
   kit hover/keyboard-focus/disabled feedback. These preserve existing behavior
   and accessible interaction, which the static export does not depict.
10. **Icons:** normal Lucide React icons replace the export's flattened Lucide
    SVG paths at the same 16px tile and 15px success sizes. Stroke rendering
    can differ from the flattened export's raster edges.
11. **License:** there is no License HTML/PNG in the supplied design set. It uses
    PageHeader, SettingsCard, SettingRow, 14px card radii, 10px controls, and S1
    colors. It shows actual plan/state/known expiry (including human-readable
    offline-access expiry) and retains activate/purchase/revalidate/manage/
    deactivate/retry actions and the guide, now in the header action slot.

No Discord link, Rust change, UI primitive edit, dependency, commit, push, or
release is included. The wider S9 all-screen polish sweep is not asserted by
this Help/License implementation.

## Verification

- Baseline: 81 frontend test files / 922 passing tests.
- Final: 81 frontend test files / 932 passing tests (+10).
- ReportProblemSection: 8 → 9 tests; AccountSection: 2 → 6 tests.
- Added wire-payload omission coverage and navigation coverage for both the
  controlled AppContainer callbacks and the local fallback.
- `pnpm typecheck && pnpm lint && pnpm test:frontend && pnpm build`: green.
- Preview shoot list includes Help and License for macOS/Windows × light/dark;
  `node --check scripts/ui-preview/shoot.mjs` passes.
- Astra review's controlled-navigation finding is fixed and regression-tested;
  follow-up source review found no remaining must-fix issue.
- Impeccable's source detector returned no findings.
- **NEEDS-VISUAL / NEEDS-SMOKE:** captures could not be produced. Installed Chrome
  aborted with SIGABRT before opening any page. Aside required unavailable
  approval; browser security policy then denied access to the local preview.
  No further browser workarounds were attempted. ±1px rendered agreement is
  therefore **not verified**; the list above covers known implementation
  deviations, not an assertion that there are no additional rendering differences.
  Neither native runtime behavior nor a Windows native artifact was verified.
