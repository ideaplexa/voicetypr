# Audio and streaming: research + implementation

`voicetypr-integration` is the working home for the Handy research and the
audio/streaming feature line. Start with
[Plan 069](../plans/069-handy-informed-audio-streaming-recovery.md) for current
status and the continuation order.

## What we want to deliver

- Live words in the recording pill, with stable confirmed text and a revisable tail.
- Fast stop-to-insertion without clipped final words, duplicate text, or lost focus.
- A smaller audio pipeline that still handles users' imported recordings.
- Model and provider choices whose actual behavior matches the UI.

## Read in this order

1. [Current recovery plan](../plans/069-handy-informed-audio-streaming-recovery.md):
   existing code, unresolved evidence, and future work.
2. [Handy synthesis](handy-teardown/00-README.md) and
   [decision review](handy-teardown/06-oracle-decision.md): product feel,
   stream contract, lifecycle, and verification lessons.
3. [Performance roadmap](voicetypr-perf/00-MASTER-ROADMAP.md) and
   [pressure test](voicetypr-perf/08-roadmap-review.md): latency and accuracy
   measurement, performance opportunities, and correctness limits.

## Historical sources, current decisions

The 15 documents under `handy-teardown/` and `voicetypr-perf/` were copied
unchanged from the research worktree on 2026-09-22. They describe an earlier
source snapshot, not today's integration branch. Their old line numbers,
external API/model claims, operational instructions, and implementation status
need revalidation before use. The referenced Handy source checkout is not part
of this import.

The old recommendation to retain FFmpeg predates this branch's in-process
decoder and libopus support. Its format-coverage concern remains useful;
the current implementation needs an explicit codec/container acceptance matrix.
Streaming is already implemented here, and later native Parakeet model routing
supersedes parts of the earlier decode-ahead handoff.

The [source manifest](research-source-manifest.json) records the original
worktree, source baseline, size, and SHA-256 of every imported document.
The source worktree and existing local benchmark artifacts were retained.

This consolidation changes documentation only. It does not establish a current
build, runtime pass, merge into main, or release.
