# Plan 070b — Decode-ahead preview edge cases (follow-up to 070)

Status: PARTIAL (2026-09-28) — item 4 shipped in 2.1.0-beta.2; items 1-3 parked
for a redesign. Attempts are on branch `wip/070b-full-attempt` (570da86a) and in
the review notes below. Source: fourth adversarial review of plan 070 (2026-09-27), no
blockers. All affect the live preview only; pasted text stays the batch
decode. Must land before the Parakeet live final may ever become the pasted
text (stop-to-text optimization).

1. **Distant repeated phrase alignment** (do first — happens in real speech):
   multi-word alignment has no temporal proximity requirement, so an earlier
   "no no" can match the committed suffix instead of its actual occurrence.
   Require temporal compatibility across the matched sequence and pick the
   closest occurrence. Harness: "no no … no no" with jitter.
2. **Failed decodes after a saved word can stall the tail**: bounded progress
   stops at the saved hypothesis word forever. Keep the saved hypothesis
   separately while allowing word-safe progress past its end. Harness: saved
   word at 0.5–0.8 s, 18 s backlog, repeated failures → finalize covers tail.
3. **Timing-less final after 15 s duplicates retained context**: reconcile the
   overlap with the frozen prefix before appending timing-less final text.
   Harness: 15 → 16 s transition.
4. **Cancel cannot interrupt finalize** (pre-existing, amplified by multi-pass
   finalize): run finalization as a cancellable task, keep session identity
   until completion, and gate `stream_final` after cancellation.

## Outcome (2026-09-28)

Item 4 (cancellable finalize, stream_final suppressed after cancel, EOF awaits
finalize) passed every review round and ships. Items 1-3 went through five
gpt-6-astra rounds; each fix created new counterexamples:

- Item 1: per-word temporal compatibility duplicated text under coherent
  +240 ms drift; a coherent-shift rule (±0.12 s around the median, |shift| ≤
  1 s) then lost genuine repeats or duplicated text when jitter is incoherent.
  Word timings alone cannot separate "same words, re-timed" from "new repeat".
- Items 2-3: freezing saved words, window-scoped reconciliation and timing-less
  finals kept producing duplicated/lost preview words in rare failure paths.

Real speech (10 clips EN/DE/ES + 52 s) was identical to plan 070 in every
variant, and all items affect only the live preview (pasted text = batch).

Redesign direction: stop re-deriving alignment from each hypothesis; keep one
running word lattice anchored to audio sample positions (committed words own
their audio span; a new hypothesis only contributes words whose audio lies
after the committed boundary, with the boundary moved only by agreement), and
add a property-based fuzz harness (random repeats + timestamp jitter) before
any rule change.
