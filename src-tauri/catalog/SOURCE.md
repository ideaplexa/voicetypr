# AI provider catalog — source & refresh

- Upstream: https://models.dev/api.json (MIT)
- Fetched: 2026-10-03
- Projected snapshot SHA256: 3e83335a8143df2043a2f07c98d0a68f7c90089f785202b8728e1c8e5a9d2b79
- Providers: 4 | text->text models: 75 (Anthropic 16, Google Gemini 19, OpenAI 36, OpenRouter 4)

## Deterministic filter (plan 017, STOP-1 tighter rule)
A provider is in the catalog IFF it has an entry in `overlay.json` mapping it to
an implemented runtime (native genai adapter or OpenAI-compatible). Native
providers include all upstream models accepting text with output exactly
`["text"]`; multimodal input is allowed, image/audio/video output is excluded.
Models marked `status: deprecated` or `retired` are excluded. OpenRouter remains
curated through `model_ids`, with labels, costs and reasoning metadata copied
from the same projected snapshot. Recommendation order comes from the overlay;
the first recommendation is the primary/default, not an alphabetical choice.
The full api.json (`.cache/`, gitignored) is NOT committed. No runtime fetches.

Older models that upstream still lists as available remain in the full picker
for saved-selection compatibility; none are primary recommendations.

## Dropped on 2026-10-03

Deprecated/retired upstream entries (including ones already excluded by output
modality):
- openai: `gpt-3.5-turbo`, `gpt-4o-2024-05-13`, `o4-mini`, `o3-mini`, `gpt-4`, `gpt-4.1-nano`, `gpt-5.2-chat-latest`, `o1`, `gpt-image-1`, `gpt-5.3-chat-latest`, `gpt-4-turbo`, `o1-pro`
- google: `gemini-3.1-flash-lite-preview`

Models missing from the refreshed upstream are also removed; see the Q3 audit
in `plans/081-q3-catalog-audit.md` for the full before/after diff and old-id audit.

## Refresh
1. Re-fetch api.json to `.cache/models.dev.api.full.json`.
2. `python generate.py --refresh` (re-projects overlay providers + used fields -> models.dev.snapshot.json).
3. `python generate.py` (snapshot + overlay -> catalog.generated.json).
4. Review diff, run `cargo test ai`, commit.
Never fetched at app runtime.
