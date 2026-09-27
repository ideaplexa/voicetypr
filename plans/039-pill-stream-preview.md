# Plan 039 — Streaming slice 3: pill committed/tentative preview (synthetic events)

> Branch feat/039-pill-stream-preview, stacked on the committed 031 pill
> (react-free vanilla TS, src/pill.tsx + pill.css). Frontend-only slice: the pill
> learns to render live committed/tentative text driven by the 037 wire format,
> proven with a synthetic event driver — BEFORE any engine emits real partials
> (oracle Step 4: the preview ships ahead of engines, verified with synthetic
> streams). Default off; production behavior pixel-identical.

## Context (verified)

- Pill entry src/pill.tsx: vanilla TS controller `createRecordingPill(root, deps)`
  with injected listen/invoke, states idle/listening/transcribing/formatting +
  error flash, DOM built once and toggled per state, tests in
  src/components/RecordingPill.test.tsx drive it via mocked listen/invoke.
- Wire format (locked by tests on the Rust branch, mirror at src/types/streaming.ts
  on feat/037-stream-contract — file does NOT exist on this branch): events on
  channel "transcription-stream", snake_case tags: started/partial/final/
  cancelled/error, fields session_id/revision/committed/tentative/text.
  RE-CREATE src/types/streaming.ts here with EXACTLY that shape (at final
  integration the two branches must produce an identical file — keep it minimal
  so the merge is trivial).

## Scope

1. **Streaming preview UI** inside the existing 260×64 pill (LAYOUT CONSTRAINT:
   do NOT resize the window in this slice — the full Handy preview panel size
   comes with the engine slice once real partials exist):
   - In the listening state, when partials arrive, a one-line preview row appears
     above the bars/timer row: `<span class="pill-committed">` +
     `<span class="pill-tentative">` as two ALWAYS-PRESENT sibling spans
     (committed normal weight, tentative dimmed/italic). The line tail-follows
     (newest text visible; overflow clipped left with a fade mask).
   - Committed span node is NEVER remounted/rewritten wholesale: append-only via
     textContent update where the new value startsWith the old (assert in code:
     if a non-monotonic committed arrives, log console.warn once and replace —
     graceful, never crash).
   - On final/cancelled/error stream events or state exit: preview row hides,
     pill returns to today's exact visuals.
2. **Gated listener**: subscribe to "transcription-stream" only when a
   `streaming_preview_enabled` setting (default false, read with the existing
   get_settings pattern) is true. Session/revision gating in TS mirroring the
   Rust gate semantics: track session_id + last revision; drop stale sessions
   and stale/duplicate revisions.
3. **Synthetic driver** (dev proof, no Rust): when `streaming_preview_demo`
   setting is true AND state enters listening, run a scripted sequence
   (setTimeout-based, injected timer deps for tests): committed grows in 4-6
   steps while tentative churns (including a rewrite of tentative, a revision
   gap, an out-of-order stale revision that must be ignored, and a stale-session
   event that must be ignored), ending with a final event. This is a dev-only
   code path — small, clearly marked, tree-shaken? (it ships but is inert;
   keep it tiny).
4. **Tests** (extend RecordingPill.test.tsx patterns):
   - committed span DOM node identity is stable across partial updates
     (same reference, text only appended).
   - tentative may rewrite freely; committed never shrinks — feed a
     non-monotonic committed and assert graceful replace + console.warn.
   - stale session and stale revision events are ignored.
   - preview hides on final/error/state-exit; baseline visuals identical when
     flag off (existing tests unmodified).
   - synthetic driver produces the full sequence under fake timers.

## Acceptance
- pnpm typecheck, pnpm lint, pnpm test pass; existing pill tests unmodified.
- pnpm build: pill entry chunk stays micro — report size; total pill JS
  (entry + preloads) must stay ≤ ~15KB.
- Flags off (default): zero behavioral difference (no listener subscribed).
- No Rust changes, no window sizing changes.
- Do not commit.
