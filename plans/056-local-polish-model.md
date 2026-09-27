# Plan 056 — Local Polish model: s1-mini first, then a trained 350M

Baseline 2026-08-20 (research session; no code changes yet).
Goal: offline, sub-second, free dictation polish; cloud only for hard cases.

## Evidence base (verified 2026-08-20)

**superwhisper/s1-mini** (HF): public weights, **Apache 2.0 + naming clause**
(card's own words; fetch exact LICENSE text before shipping). GGUF builds at
`superwhisper/s1-mini-GGUF` (llama.cpp-ready). 596M params, 462 MiB quantized,
"runs comfortably on a laptop CPU", 94.8% token accuracy on 7,519 held-out
English cases. English only, v1. Input contract: exact system prompt +
control line + raw transcript; `enable_thinking=False` required (else empty
output). ~1,000-token recommended input — chunk longer dictations. Pure
filler → empty string (maps to our unchanged/empty outcome path).

Control line ↔ our modes:
`[Styling: casual|semi-casual|semi-formal|formal] [Structure: prose|lists]
[Context: general|email]` — Clean Dictation ≈ semi-formal/prose/general.

**LFM2.5-350M** (training target): IFEval 76.96 — beats Qwen3.5-0.8B
Instruct (59.94) at <half the params; 313 tok/s CPU decode, <1GB, 9
languages, 32K context; `-Base` checkpoint + Unsloth/TRL SFT paths.
**lfm1.0 license: free commercial <$10M revenue (entity + affiliates)** —
decide before bundling. **Qwen3.5-0.8B**: Apache-2.0 but IFEval 52.1 +
vision-encoder overhead → fallback only.

## Phase A — integrate s1-mini (no ML work)

1. Runtime decision: bundled llama.cpp (their GGUF, day-one) vs MLX convert
   (Qwen3 arch converts via mlx-lm). macOS-first; Windows story required
   before cross-platform claim.
2. Download via existing Models-tab machinery (Parakeet precedent), ~462 MiB.
3. Gating: English + Clean-Dictation-class modes → s1-mini; everything else
   falls back to the configured provider. Never blocks on model download.
4. Implement the exact input contract (fixed system prompt + control line,
   thinking-off prefix); empty output = no-op.
5. Attribution per the naming clause (credit "S1-mini by Superwhisper").

## Phase B — train LFM2.5-350M on our corpus (separate claim)

Distillation corpus = history pairs (raw transcript → polished output, mode,
provider). SFT/LoRA per Liquid's documented path. Ship as a second
downloadable model only if eval on real multilingual transcripts beats
s1-mini AND the lfm1.0 cap is accepted; otherwise train Qwen3-0.6B/3.5-0.8B
for Apache purity and keep LFM as user-side download.

## STOP conditions

- Exact s1-mini LICENSE wording unresolved → do not bundle weights.
- Phase A latency on target hardware >500ms end-to-end → re-evaluate runtime.
- Phase B quality/eval inconclusive → stay on Phase A + cloud tiers.

## Acceptance

Phase A: offline dictation with polish completes <500ms on an M-class CPU,
zero network in the polish stage, gates green, CUA smoke of download +
dictate + polish. Phase B: beats s1-mini on our held-out multilingual eval
before any shipping decision.
