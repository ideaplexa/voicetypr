# Plan 037 (draft) — Streaming slice 1: engine-agnostic event contract

> First slice of the streaming pill program (task #6), per
> docs/handy-teardown/06-oracle-decision.md (research worktree). Contract + gating
> helpers + tests ONLY — no engine emits these events yet, no UI consumes them yet.
> This slice must be inert in the shipping app (types + tested helpers).

## Contract (from the oracle decision, Q3)

New module src-tauri/src/transcription/stream.rs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptionStreamEvent {
    Started   { session_id: u64, engine: String, revision: u64 },
    Partial   { session_id: u64, revision: u64, committed: String, tentative: String },
    Final     { session_id: u64, revision: u64, text: String },
    Cancelled { session_id: u64, revision: u64 },
    Error     { session_id: u64, revision: u64, error: String },
}
```

Plus:
- `EngineStreamCapabilities { supports_streaming, supports_committed_prefix,
  supports_tentative_tail, supports_endpointing, final_only }` — const per engine
  (whisper/parakeet/cloud/remote all `final_only: true` TODAY).
- `StreamSessionGate`: tracks active session_id + last revision; methods
  `admit(&event) -> Admit { Accept, StaleSession, StaleRevision }` and
  `assert_committed_monotonic(prev_committed, next_committed) -> bool`
  (next must start_with prev — grapheme-safe: compare on char boundaries, and the
  helper must reject a "committed" that shrinks or rewrites).
- Integrate with the existing generation discipline: session_id SOURCE is the
  existing RECORDING_GENERATION pattern in commands/audio.rs (oracle: audio.rs:62-164
  `persist_if_current`/`delivery_aborted`) — do NOT invent a parallel counter; expose
  a constructor that takes the current generation. Verify the actual code before
  wiring.

TS mirror src/types/streaming.ts: the event union + capability type + the Tauri
event name constant (`transcription-stream`) — types only, no listener.

## Tests (the point of this slice)

Rust unit tests in stream.rs:
1. stale session dropped (gate on session N, event from N-1 → StaleSession)
2. revision ordering (revision <= last → StaleRevision; gaps allowed forward)
3. committed monotonicity: growing prefix accepted; shrinking/rewriting rejected;
   multi-byte (é, 日本語, emoji) boundaries never split
4. final-only engine shape: capabilities for all current engines report
   final_only=true (locks today's behavior into the contract)
5. serde round-trip for every variant (snake_case tag names locked — these become
   the wire format the frontend sees)

## Acceptance
- cargo test passes (existing 1185 + new); pnpm typecheck passes (TS types compile).
- Zero behavior change: no call sites besides tests; module compiled but unused
  (allow dead_code at module level with a comment pointing at the program plan).
- Do not commit.

## Follow-on slices (for context, not this slice)
038: FIFO streaming substrate (no-op worker: Feed/Finalize/Cancel one queue, engine
lease, PCM tap behind a flag) → 039: pill committed/tentative preview driven by
synthetic events behind a dev flag → 040: Parakeet SlidingWindow-vs-EOU measured
vertical → cloud WS.
