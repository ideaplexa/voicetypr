# Plan 050: Navigation ownership reorganization

**Status:** DONE — frontend gates and native macOS CUA smoke passed 2026-08-17
**Priority:** P1
**Effort:** M
**Baseline:** `894ecffd`
**Depends on:** 047

## Goal

Make each sidebar destination answer one clear user question without collapsing independent capabilities merely to reduce navigation count.

## Product decisions

- Keep Sources and Network sharing separate. Sources chooses where this device transcribes; Network sharing makes this device available to others. Both can be active simultaneously.
- Keep Polish capabilities together and retain the established `Dictionary` label. Use a vertical workflow rail so provider, vocabulary, replacements, snippets, and writing modes have explicit purposes without becoming sidebar destinations.
- Create a dedicated Recording destination that owns the full before/during/after recording workflow.
- Order direct navigation by workflow and frequency: Overview, General, History, Upload, Sources, Recording, Polish, Shortcuts, Network sharing, CLI, License, Diagnostics.
- Keep CLI in primary navigation because it is a product capability; label it plainly and provide a reusable prompt for Claude Code, Codex, OpenCode, OpenClaw, and other terminal agents.
- Keep General, License, and Diagnostics as direct sidebar destinations. A three-item Settings modal adds navigation cost without reducing complexity.
- Keep license state visible in the app header and deep-link the badge directly to License.
- Keep Report a problem fixed at the bottom as a distinct action.

## Scope

1. Add a `recording` screen and route with a dedicated Recording page.
2. Move capture controls, transcript handling, audio feedback, transcription performance, recording indicator, storage, and cleanup from General Settings to Recording without duplicating editors.
3. Keep Appearance, App behavior, and privacy controls in General.
4. Keep permissions, troubleshooting, and reset/start-over controls in Diagnostics; keep reset controls out of License.
5. Present every product and settings destination in one compact, non-scrolling sidebar list.
6. Preserve license discoverability through a visible header badge, attention treatment, and direct License navigation.
7. Reorder primary navigation and rename Agent & CLI to CLI.
8. Add agent-oriented CLI onboarding with a copyable, privacy-preserving transcription prompt.
9. Replace Polish's dense horizontal tab strip with an explicit vertical workflow rail and descriptive labels.
10. Clarify that Shortcuts owns additional actions while Recording owns the primary recording trigger.
11. Reduce excess titlebar, brand-row, and page-header spacing while preserving macOS traffic-light clearance and toggle alignment.
12. Update affected tests and preserve existing state/update behavior.

## Out of scope

- Combining Sources with Network sharing.
- Renaming Dictionary.
- Splitting Polish into additional sidebar destinations.
- Backend settings or persistence changes.

## Verification

- Focused component tests cover direct navigation order, fixed reporting, compact chrome, Recording content, General content, License content, Diagnostics/reset ownership, CLI onboarding, Polish workflow navigation, and shortcut guidance.
- `pnpm typecheck`
- `pnpm lint`
- `pnpm vitest run`
- Native Tauri smoke confirms the all-at-once sidebar, fixed Report a problem action, direct General/License/Diagnostics routes, compact page spacing, aligned titlebar toggle, Recording ownership, the two-column Polish workflow, and CLI agent onboarding.

## Evidence

- Focused navigation/settings suite: 5 files, 26 tests passed.
- Frontend gates: ESLint passed, TypeScript passed, 62 files / 664 tests passed.
- Native macOS CUA smoke: verified every destination is visible without sidebar scrolling; Overview remains first and General second; Report a problem stays fixed at the bottom; General, License, Diagnostics, and reporting navigate directly; reset tools live in Diagnostics; page headers and the brand row use compact top spacing; the titlebar toggle aligns with the traffic lights and still collapses/expands the sidebar.
