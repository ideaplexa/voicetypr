# AI provider catalog — source & refresh

- Upstream: https://models.dev/api.json (MIT)
- Fetched: 2026-10-03; merged 2026-10-10 effort metadata and Claude Haiku 5.5 from stable
- Projected snapshot SHA256: 8c2960cce6857956cd9823589be557aeb8b54365a395a47c623aa0458953130f
- Providers: 4 | text->text models: 71 (anthropic 17, gemini 17, openai 33, openrouter 4)

## Deterministic filter (plan 017, STOP-1 tighter rule)
A provider is in the catalog IFF it has an entry in `overlay.json` mapping it to
an implemented runtime (native genai adapter or OpenAI-compatible). Native
providers include all upstream models accepting text with output exactly
`["text"]`; multimodal input is allowed, image/audio/video output is excluded.
Specialist embedding, rerank, TTS, image, audio and moderation IDs are excluded
even when upstream labels their output as text; modality metadata must still
allow text input and text-only output. Multimodal chat input remains allowed.
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

## Fix-round specialist exclusions (2026-10-03)

- gemini: `gemini-embedding-001`
- gemini: `gemini-embedding-2`
- openai: `text-embedding-3-large`
- openai: `text-embedding-3-small`
- openai: `text-embedding-ada-002`

## Native request contracts

- Effort choices are capped to `none`, `minimal`, `low`, `medium`, `high`.
  Effort options intersect that ladder; toggles add `none`; budget-only options
  expose low/medium/high, with `none` when toggled or their minimum is zero.
  Saved choices resolve to the selected level, medium, the highest level below
  medium, then the lowest supported level, in that order.
- Native OpenAI uses Responses with `store: false`. Fast requests use
  `service_tier: "fast"` ([OpenAI fast mode](https://openai.com/api-fast-mode)).
- New Claude families use raw Messages with `output_config.effort` and adaptive
  thinking. Toggle `none` disables thinking and omits effort. Sonnet 5.5's
  below-low request uses `between_tools`, not disabled thinking, as documented
  in the [thinking configuration table](https://platform.claude.com/docs/en/build-with-claude/thinking).
  Fast Opus models add `speed: "fast"` and `anthropic-beta: fast-mode-2026-02-01`
  ([Claude fast mode](https://platform.claude.com/docs/en/build-with-claude/fast-mode)).
- Gemini 2.5 `none` uses an explicit zero budget: genai's `None` variant omits
  thinking configuration and does not disable it. Legacy medium/high budgets
  are 8000/24000 tokens; max output includes a larger thinking allowance and is
  clamped to the catalog output limit.
- Key validation uses the provider default at its lowest supported effort and
  standard speed, without modifying preferences. Retired native selections
  migrate to the provider default instead of requiring reselection.

Main-line model recommendations and specialist/deprecated exclusions are preserved.
CLI defaults remain Sonnet/low for Claude, CLI-owned models/thinking-off for pi/omp,
and GPT-6 Luna/medium/Fast for Codex; stable defaults do not override them.
