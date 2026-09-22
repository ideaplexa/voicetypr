# Plan 069 — Handy-informed audio and streaming recovery

Status: IN PROGRESS — claimed Codex 2026-09-22.

## Authorized outcome

Bring the existing Handy teardown and performance research into
`voicetypr-integration`, preserve its source documents, and reconcile its
recommendations with the implementation already on `feat/049-pure-rust-audio`.
Create a current continuation plan that distinguishes existing code, runtime
verification, main-branch reconciliation, and future product opportunities.

This claim covers research consolidation and planning. Product implementation,
merging the old feature line, publishing, and release sign-off are not completed
by this claim.

## Baselines

- Integration application code: `58ec176adc34e4e03ac9b3930dade68f13acd43f`.
- Main: `c47e1465f7ef34ac2ca31b5b5b226981b41b17ab`.
- Research worktree: `research/handy-teardown` at `af63ab13`, with 15 untracked
  Markdown documents under `docs/handy-teardown` and `docs/voicetypr-perf`.

## Completion criteria

- Preserve all 15 source documents byte-for-byte in the integration worktree.
- Add a current entry point and a research-to-implementation disposition map.
- Record known contradictory/stale evidence and concrete continuation stages.
- Keep existing local benchmark files, corpus, and handoff untouched.
- Check copied-document hashes, current-source references, Markdown links in
  authored documents, whitespace, and the final scoped Git diff.
