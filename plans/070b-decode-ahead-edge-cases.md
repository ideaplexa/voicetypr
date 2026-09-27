# Plan 070b — Decode-ahead preview edge cases (follow-up to 070)

Status: TODO. Source: fourth adversarial review of plan 070 (2026-09-27), no
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
