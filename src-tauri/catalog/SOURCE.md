# AI provider catalog — source & refresh

- Upstream: https://models.dev/api.json (MIT)
- Fetched: 2026-10-10
- Projected snapshot SHA256: f203a3ff6aa9b460c67bfc83431c63cccdb54f3c09f8d16470019a43282ed173
- Providers: 3 | models: 109

## Deterministic filter (plan 017, STOP-1 tighter rule)
A provider is in the catalog IFF it has an entry in `overlay.json` mapping it to a
genai 0.6 `AdapterKind` (the runnable-via-genai set). Models are pulled
automatically from the pinned snapshot (every model listed for that provider).
`overlay.json` also pins, per provider, the ordered `recommended` list (index 0
is the provider default) and the `fast_models` list that marks models eligible
for the provider's fast service tier.
The full 226-provider api.json (`.cache/`, gitignored) is NOT committed.

## Refresh
1. Re-fetch api.json to `.cache/models.dev.api.full.json`.
2. `python generate.py --refresh` (re-projects overlay providers + used fields -> models.dev.snapshot.json).
3. `python generate.py` (snapshot + overlay -> catalog.generated.json).
4. Review diff, run `cargo test ai`, commit.
Never fetched at app runtime.

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
