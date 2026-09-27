# Plan 055 — Polish latency: fast-path defaults + honest speed guidance

Baseline 2026-08-20 (measured on user's machine, M4 Pro; no code changes yet).
Goal: make the fast path the default and make "which model is fast" answerable
from the UI, without curating model lists.

## Measured evidence (do not re-derive)

| Probe | Result |
|---|---|
| pi polish, thinking off, 93ch | 7,719 ms (was 9.8s avg with thinking) |
| omp polish, fast + thinking off | 6,376 → 8,800 → 9,476 ms (variance, not flag drift) |
| omp telemetry for "hello" | input 42,861 tokens, TTFT 5,296 ms, gen 492 ms, $0.032/call |
| identical omp call repeated | cacheRead 0, TTFT still ~4.9s — **no prompt caching** |
| CLI boot floors | pi 1.0s, omp 0.25s (`--help`) |
| claude -p minimal probe | errored fast (auth/alias) — not a data point |

Law: agent CLIs inject their full agent context (~42K tokens) regardless of
`--no-*` flags; prefill of that backpack is the fixed cost. Streaming is
pointless for CLIs (output is ~0.5s of a ~6.2s call). Direct API with our
~400-token prompt is the fast path; CLIs are the zero-setup convenience tier.

## Deliverables

1. **Fast default**: when a Cloud (direct-API) provider is configured and the
   user hasn't pinned a model, default to that provider's nano/mini/flash/
   haiku-class model + thinking off. Selector-class heuristic on catalog ids;
   no list surgery.
2. **Guidance, not curation**: in `ProviderSetupDialog`, fast-class model rows
   get a "⚡ Small & fast — recommended for dictation" hint; CLI rows get
   honest "~5s typical" copy. No filtering, no hiding.
3. **Measured-latency chips**: average `ai_polish` duration from history per
   (provider, model) rendered on rows — "≈6.4s · your last 5 runs"; "not used
   yet" otherwise. Data already logged per run.
4. Optional: record `duration_ms` per (provider, model) into a rolling stats
   table if history queries prove too coarse at render time.

## STOP conditions

- Anything touching executor/runtime policy (timeout, retry, cancel, error
  mapping) belongs to the 016-owned modules — split out if needed.
- If chip queries regress dialog open time, ship 1+2 first and gate 3 behind
  a cached stats read.

## Acceptance

Gates green (`pnpm check`); CUA smoke of the dialog showing hint + chips with
real history; a fresh Cloud-provider enable lands on a small/fast model by
default; no behavior change for users with an explicit model pinned.
