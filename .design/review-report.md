# Voicetypr — Full UI/UX Review

Date: 2026-08-14 · Scope: all 12 sidebar surfaces + title bar + share-stats modal (native Tauri build, current worktree)
Method: live walkthrough of the running app via cua-driver (9/12 screens captured to `.design/ev-*.png`, rendered at 1000×680), plus code-level audit of state coverage, tokens, type scale, and copy. Three screens (Account, Diagnostics, Report a problem) were code-reviewed but not visually captured — see Verification gaps.

## Executive read

This is a coherent, restrained system: one accent, one card language, plain-language copy, and a sidebar whose hierarchy finally matches the mental model (work surfaces on top, utility at the bottom). The two findings that most affect users are trust-level, not cosmetic: the Overview chart fabricates bars for users with no data, and history loads fail silently everywhere — a user with a broken database and a brand-new user see the exact same screen.

## What works and should be preserved

- **Single flat navigation + static utility footer.** The navScreens/footerNavScreens split reads instantly; active item treatment (card + ring + sage icon) is consistent between the two groups.
- **Settings-card system.** Icons, title, description, SettingRow alignment (`ml-[26px]` desc-indent) is applied uniformly across Settings, Shortcuts, Polish, Network sharing, Diagnostics.
- **State coverage depth where it exists.** NetworkSharingCard alone handles 8 states (server off, no shareable model, current-model-unshareable, remote-in-use, firewall, no-address, failed bindings, ready). ModelsSection, AudioUploadSection, and RecentRecordings are comparably resilient.
- **Copy voice.** "Ready for the next recording", "Your voice, in numbers", telemetry's "No audio, transcripts, or personal data — ever." Specific, unpretentious, no AI-filler.
- **Color discipline.** Sage is the only chroma of consequence; status colors are always paired with icons and labels (no color-only badges). Dark/light tokens are genuinely two palettes, not an inversion.

## Scorecard

| Dimension | Score | Note |
|---|---|---|
| Product clarity | 3 | Sidebar + Overview hero make the job obvious in the first viewport |
| Hierarchy & composition | 3 | One dominant region per screen; sparse screens (Network sharing) expose the pattern |
| Task flow & feedback | 2 | Silent-load failures; some actions float in dead space (Agent/CLI) |
| Typography | 2 | Two coexisting scales (Tailwind steps + off-scale 12.5/13/13.5/15px set) |
| Color roles & contrast | 3 | Two marginal light-mode spots: sage-on-sage-bg ≈4.7:1; 10px license badge ≈4.6–5.0:1 |
| Component consistency | 3 | SettingsCard everywhere; generic `Card` is a second pattern used in few places |
| Empty/loading/error | 2 | Excellent coverage in 7 screens; absent where it matters most (History/Overview data layer) |
| Credibility | 3 | Specific copy and real stats — undermined by the fabricated zero-data chart |

## Findings, by severity

### P1 — Trust defects

**F1. Overview chart fabricates data for fresh users.**
Evidence: `src/components/tabs/OverviewTab.tsx:271-274` — `heightPct = Math.max(6, ...)` renders all seven bars at 6% height when `weekMax` is 1 and every day is 0 (screenshot looks identical to a quiet week). A new user sees a plausible-looking pattern of activity that never happened.
Impact: new users' first read of the app's proof-of-value is wrong; if noticed, poisons the whole dashboard's credibility.
Cause: guard was written to keep bars clickable/aligned, not to represent zero.
Prescription: when `weekCount === 0`, replace the bar row with a single empty state inside the same card ("No transcripts this week — your daily activity will chart here.") and keep the summary caption. Do not render stub bars.
Confidence: high (code + rendered evidence).

**F2. History data failures are invisible app-wide.**
Evidence: `src/hooks/useTranscriptionHistory.ts:47-56, 74-76` initializes to `[]` and only `log.error`s on load failure. No `isLoading`/`isError` in the return. OverviewTab and RecordingsTab therefore render the identical zero-state for fresh / loading / failed.
Impact: a user with a corrupted store sees what looks like a wiped history; support tickets will read "my transcripts disappeared" with no user-facing hint.
Cause: hook predates the state-coverage pass that hardened the other screens.
Prescription: extend the hook with `isLoading`/`loadError`; Overview renders skeleton bars while loading, History renders an error banner with retry ("Couldn't load history – Retry").
Confidence: high (code; states verified absent in `RecentRecordings.tsx` and `OverviewTab.tsx`).

### P1 — Comprehension defects

**F3. Model path truncation crowds the toggle.**
Evidence: `.design/ev-polish.png` and `.design/ev-sources.png` — truncated path `/Volumes/1tb-drive/lm-studio/models/mlx…` runs to the switch's left edge with no breathing room in provider cards.
Impact: settings row looks broken in its highest-traffic variant (custom provider path).
Cause: flex row without `min-w-0`/`truncate` discipline between the path text and the trailing Switch.
Prescription: constrain the path span with `min-w-0 truncate` and `mr-3` before the control.
Confidence: medium (rendered; exact line to be confirmed in ModelCard/provider card during fix).

**F4. Settings hotkey row duplicates its value.**
Evidence: `.design/ev-settings.png` — "Tap Left ⌥ to toggle" appears as both the field's supporting text and the pill next to Edit.
Impact: reads as a rendering bug on the app's primary configuration control.
Prescription: keep the pill (the value); make the supporting text explain the behavior ("Primary recording shortcut") or drop it.
Confidence: high (rendered).

**F5. Repeated value: total words shown twice in one viewport.**
Evidence: Overview — caption under bars (`16,296 words`) and the dark stats card (`16,296`) say the same number twice in the same scan.
Impact: mild; dilutes the card's single-number emphasis.
Prescription: drop `words` from the caption line (keep today · 7 days · avg/transcript) — the dark card already owns "words captured".
Confidence: high.

### P2 — Consistency / fit

**F6. Network sharing repeats its concept three times.**
Evidence: `.design/ev-network.png` — heading description, card title "Remote Transcription", and body "When enabled, another Voicetypr app can use this device's…" all state the same fact.
Prescription: heading gets the concept; card body becomes operational ("Shareable now · Parakeet V2" or the off state).
Confidence: high.

**F7. Agent & CLI card actions float in dead space.**
Evidence: `.design/ev-agent.png` — "Repair command / Remove command" sit bottom-right of a mostly empty card, visually detached from what they affect (the "Ready and compatible" status).
Prescription: right-align them in the "Ready and compatible" row itself; the empty card interior collapses.
Confidence: high.

**F8. Two type scales coexist.**
Evidence: Tailwind steps (10/12/14/16/18) across nav/list surfaces vs. the settings primitives' fractional set (12.5/13/13.5/15, 1.75rem) in `src/components/settings/settings-ui.tsx`.
Impact: invisible side-by-side, visible when surfaces mix (History meta row vs. settings row atop the same screen).
Prescription: one scale: pick the fractional set's nearest standard steps (13→14 for body, 15→16 for card titles) and converge `settings-ui`.
Confidence: medium.

**F9. Footer nav is inset 1px–4px off from the main nav.**
Evidence: `SidebarContent` is `px-2`; the footer `<nav>` lives inside `SidebarFooter px-3` — icons in the two groups don't share a left axis (visible in `.design/ev-overview.png` if zoomed).
Prescription: align both groups to the same padding; put the version row on the footer axis too.
Confidence: medium (pixel-level).

**F10. Light-mode contrast is marginal in two small places.**
Evidence: sage text on sage-bg ≈4.7:1 (AA-pass, AAA-fail); the 10px sidebar license badge ≈4.6–5.0:1 (NavChromaScout).
Prescription: darken sage text one step in light mode for ≤12px text, or bump badge to 11px + slightly darker sage.
Confidence: medium (computed, not rendered zoom-tested).

### P3 — Polish

- **F11.** Polish runtime errors surface only as toasts (`EnhancementsTab.tsx:17-30`); a persistent failure (bad API key) deserves an inline banner on the Polish card so the fix location is where the error appears.
- **F12.** Sources screen ambiguity: Local tab count (6) vs. visible downloaded set (2) — unclear what the numbers count; add a label ("6 available · 2 downloaded").
- **F13.** `report-a-problem`, Diagnostics, and Account look correct at code level post-refactor, but need one visual pass on the final UI (see gaps).
- **F14.** The app exited (code 0) during the walkthrough while Agent & CLI was open — likely user-side quit of the wrapper, but worth one manual re-check that screen can't trigger termination.

## Action plan

> **Status 2026-08-14:** F1, F3, F4, F5, F6, F7 implemented; F3–F7 verified in the running app (`.design/after-*.png`). F1 is code-verified only — this account has data this week, so the zero-state can't render live.

> **Status 2026-08-14 (Next block):** F2, F8, F9, F10, F12 implemented; reviewer passed with two P2s (hook race, missing UI tests), both fixed — generation counter in the hook + race regression test, three new load-state tests. F8–F12 verified live (`next-*.png`). Remaining: F11 (inline Polish error banner), F13/14 visual pass, dark-theme + zoom sweep.

> **Status 2026-08-14 (Later block):** F11 implemented — persistent Polish failures now show an inline amber banner on the Polish card (Zustand-backed so it survives tab switches; toasts unchanged), 5 new tests. F13 verified live: Account (reset card present), Diagnostics (Permissions → Quick fixes → telemetry), Report (single form card, no quick-fix/system-spec blocks) — `gap-*.png`. F14: the app exited cleanly during walkthrough; attributed to user-side quit, not the screen. New finding **F15 (P3): the `theme` store field has no UI control anywhere** — users cannot switch Light/Dark; dark-theme capture remains blocked on this (flipping macOS appearance system-wide is too invasive to do silently). 200%-zoom remains untested (window min-size 1000×680 is the floor).

> **Status 2026-08-14 (F15):** Resolved, and upgraded in scope — the report understated it: nothing ever applied `.dark` to the document, so dark mode was entirely dead code, not just picker-less. Fix: `useTheme` hook + `ThemeSync` in App (system/light/dark with live `matchMedia` tracking, unknown values → system) + Appearance card with Theme select in Settings. Verified live: the running app rendered **dark for the first time** the moment the hook mounted (OS preference was already dark — the app had simply never honored it; `gap-theme-dark-applied.png` etc.). 13 new tests. Remaining nit: committing the Radix popup via synthetic CUA events doesn't land (driver/portal quirk, not an app defect) — human click works; explicit-Dark selection is covered by unit tests instead.

**Now (this session, small diffs):**
1. F1 empty-state swap in Overview chart card.
2. F4 dedupe the hotkey row copy. F5 drop duplicate words in caption.
3. F6/F7 Network + Agent/CLI copy/action placement.
4. F3 min-w-0/truncate on model path.

**Next:**
5. F2 hook loading/error propagation + banners (touches hook + two consumers).
6. F8 type-scale convergence; F9 nav axis alignment; F10 contrast bumps.
7. F12 Sources count labels.

**Later:**
8. F11 inline Polish error banner; F13/14 visual pass once the dev server is running; 200%-zoom and dark-theme capture sweep.

## Verification gaps & assumptions

- Account, Diagnostics, Report a problem reviewed from code (recent by this session's own edits); rendered evidence pending — their screenshots weren't captured before the wrapper exited.
- Dark theme not captured this session; token audit is from CSS values, not rendered contrast.
- No zoom (>100%), narrow-window, RTL, or keyboard-only traversal performed.
- `minW/minH 1000×680` is the only real breakpoint in this desktop app; "responsive" findings were read as "min-size" findings accordingly.
