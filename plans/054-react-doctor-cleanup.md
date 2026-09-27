# Plan 054 — react-doctor full-court cleanup

Baseline after 052/053: 62/100 ("Needs work"), 93 findings.
Goal: clear every mechanical finding; split giant components only where the
score demands it. Behavior-preserving; tests after each wave.

## Waves (parallel, file-disjoint)

1. **HookBugHygiene** — impure state updater (critical), loading flags in
   finally, exhaustive-deps, set-state-after-await, lazy ref init,
   pure-fn hoists, only-export moves, RecentRecordings a11y.
2. **PerfContextStability** — context value memoization (License,
   Settings, toggle-group), noopener, fetch status check, Set/Map lookups,
   constant-JSX hoist, ui-primitive variant exports to sibling modules.
3. **SectionsPerfA11y** — stable list keys, transition-all, accessible
   labels, combined iterations, Promise.all for independent awaits.

## After waves

- Re-run react-doctor; decide how far to push `no-giant-component` ×14
  (each split is a plan-051-style refactor — only do what the score needs).
- Full `pnpm vitest run`, `pnpm typecheck`, `pnpm lint`, `pnpm build`.
- Live CUA smoke of the touched surfaces.

## Acceptance

Score materially raised from 62; zero regressions in the 663-test suite;
finding-by-finding accounting (fixed / skipped-because-false-positive).

## Outcome (2026-08-19)

react-doctor **62 → 89 / 100 ("Great")**; 93 → 1 finding. The last finding
(RemoteServerCard exhaustive-deps) is a verified false positive — its
dependency arrays are complete.

- Waves 1–3 (delegated): keys, refs, a11y, context stability, pure-fn hoists,
  iteration combining, variant-export moves.
- Wave 4 (delegated, 5 agents): 15 giant components split into ~60 focused
  modules (largest now <280 lines; OnboardingDesktop 1515→128 orchestrator,
  RecentRecordings 1079→89, ModelsSection 1103→246, NetworkSharingCard
  872→73, HotkeyInput 488→63).
- Parent pass: unconditional finally resets, MicrophoneSelection validation
  folded into device sources, share-card stats narrowed, availability props
  grouped, listener resubscribe churn removed.

Gates: typecheck, lint, build green; 663/663 tests; live CUA click-through
of Overview/Sources/History/Recording/Polish/Network + pixel sanity.
