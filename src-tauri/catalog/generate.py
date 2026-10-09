#!/usr/bin/env python3
"""Generate the embedded AI provider catalog from the pinned models.dev snapshot."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
SNAPSHOT_PATH = ROOT / "models.dev.snapshot.json"
OVERLAY_PATH = ROOT / "overlay.json"
CATALOG_PATH = ROOT / "catalog.generated.json"
FULL_API_PATH = ROOT / ".cache" / "models.dev.api.full.json"

STATUS_ORDER = {"production": 0, "experimental": 1, "hidden": 2}
# Canonical low→high ladder the app exposes. models.dev effort values are
# intersected with this set (xhigh/max are never surfaced).
NATIVE_EFFORT_LEVELS = ("none", "minimal", "low", "medium", "high")
REASONING_ADAPTERS = {"OpenAI", "OpenAIResp", "Anthropic", "Gemini"}
PROVIDER_FIELDS = ("id", "name", "env", "npm", "doc", "api")
MODEL_FIELDS = (
    "id",
    "name",
    "family",
    "reasoning",
    "reasoning_options",
    "tool_call",
    "structured_output",
    "modalities",
    "limit",
    "cost",
    "release_date",
    "last_updated",
)


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, value: Any) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def humanize(identifier: str) -> str:
    words = identifier.replace("_", "-").split("-")
    return " ".join(word.upper() if word.isdigit() else word.capitalize() for word in words if word)


def sorted_models(models: Any) -> list[dict[str, Any]]:
    if isinstance(models, dict):
        entries = list(models.values())
    elif isinstance(models, list):
        entries = models
    else:
        return []
    return sorted(
        (entry for entry in entries if isinstance(entry, dict)),
        key=lambda entry: str(entry.get("id", "")),
    )

def reasoning_levels(model: dict[str, Any]) -> list[str]:
    """Per-model selectable reasoning levels for the native UI/runtime.

    - Effort-value models: catalog values intersected with the native ladder;
      a "toggle" option adds "none" (thinking can be switched off).
    - Budget-only models (budget_tokens, no effort values): low/medium/high,
      mapped onto genai's legacy budgets; "none" when a toggle exists or the
      budget minimum is 0.
    - Non-reasoning models: no levels.
    """
    if not model.get("reasoning", False):
        return []
    options = [
        option
        for option in (model.get("reasoning_options") or [])
        if isinstance(option, dict)
    ]
    has_toggle = any(option.get("type") == "toggle" for option in options)
    budget_option = next(
        (option for option in options if option.get("type") == "budget_tokens"), None
    )

    effort_values = [
        value
        for option in options
        if option.get("type") == "effort"
        for value in option.get("values", [])
    ]
    if effort_values:
        levels = [level for level in NATIVE_EFFORT_LEVELS if level in effort_values]
        if has_toggle and "none" not in levels:
            levels.insert(0, "none")
        return levels

    if budget_option is not None:
        levels = list(NATIVE_EFFORT_LEVELS[2:])
        if has_toggle or budget_option.get("min") == 0:
            levels.insert(0, "none")
        return levels

    return []



def project_model(
    model: dict[str, Any], recommended_ids: set[str], fast_ids: set[str]
) -> dict[str, Any]:
    model_id = str(model.get("id") or model["model_id"])
    projected: dict[str, Any] = {
        "model_id": model_id,
        "label": str(model.get("name") or model.get("label") or humanize(model_id)),
        "recommended": model_id in recommended_ids,
        "reasoning": bool(model.get("reasoning", False)),
        "reasoning_levels": reasoning_levels(model),
        "fast_mode": model_id in fast_ids,
    }
    projected["reasoning_efforts"] = [
        value
        for option in model.get("reasoning_options") or []
        if isinstance(option, dict) and option.get("type") == "effort"
        for value in option.get("values", [])
    ]

    limit = model.get("limit")
    if isinstance(limit, dict):
        if isinstance(limit.get("context"), (int, float)):
            projected["context"] = limit["context"]
        if isinstance(limit.get("output"), (int, float)):
            projected["output"] = limit["output"]

    cost = model.get("cost")
    if isinstance(cost, dict):
        if isinstance(cost.get("input"), (int, float)):
            projected["cost_input"] = cost["input"]
        if isinstance(cost.get("output"), (int, float)):
            projected["cost_output"] = cost["output"]

    return projected


def build_catalog() -> dict[str, Any]:
    snapshot_bytes = SNAPSHOT_PATH.read_bytes()
    snapshot = json.loads(snapshot_bytes.decode("utf-8"))
    overlay = load_json(OVERLAY_PATH)

    providers: list[dict[str, Any]] = []
    for provider_id, config in overlay.items():
        if provider_id == "custom":
            continue

        snapshot_id = config.get("snapshot_id")
        inline_models = config.get("models")
        provider = snapshot[snapshot_id] if snapshot_id else {}
        recommended_ids = set(config.get("recommended", []))
        adapter = config.get("adapter")
        fast_ids = set(config.get("fast_models", []))
        source_models = inline_models if inline_models is not None or snapshot_id is None else provider.get("models")
        models = [
            project_model(model, recommended_ids, fast_ids)
            for model in sorted_models(source_models)
        ]
        # The overlay recommended list is an ordered preference list: index 0
        # is the provider default. Recommended models sort by that order, then
        # all other models alphabetically.
        recommended_order = list(config.get("recommended", []))
        models.sort(
            key=lambda model: (
                recommended_order.index(model["model_id"])
                if model["model_id"] in recommended_ids
                else len(recommended_order),
                model["model_id"],
            )
        )
        runtime = str(config.get("runtime", "genai_adapter"))

        providers.append(
            {
                "id": provider_id,
                "label": str(config.get("label") or provider.get("name") or humanize(provider_id)),
                "status": config["status"],
                "runtime": runtime,
                "adapter": adapter,
                "namespace": config.get("namespace"),
                "requires_api_key": True,
                "supports_base_url": False,
                "supports_reasoning": False if inline_models is not None or snapshot_id is None else adapter in REASONING_ADAPTERS,
                "docs_url": provider.get("doc"),
                "models": models,
            }
        )

    providers.sort(key=lambda provider: (STATUS_ORDER[provider["status"]], provider["id"]))
    return {
        "generated_from_sha": hashlib.sha256(snapshot_bytes).hexdigest(),
        "providers": providers,
    }


def project_snapshot() -> None:
    full = load_json(FULL_API_PATH)
    overlay = load_json(OVERLAY_PATH)
    projected: dict[str, Any] = {}

    for config in overlay.values():
        snapshot_id = config.get("snapshot_id")
        if snapshot_id is None:
            continue
        provider = full[snapshot_id]
        projected_provider = {
            key: provider[key] for key in PROVIDER_FIELDS if key in provider
        }

        source_models = provider.get("models", {})
        projected_models: dict[str, Any] = {}
        for model in sorted_models(source_models):
            model_id = str(model["id"])
            projected_models[model_id] = {
                key: model[key] for key in MODEL_FIELDS if key in model
            }
        projected_provider["models"] = projected_models
        projected[snapshot_id] = projected_provider

    write_json(SNAPSHOT_PATH, dict(sorted(projected.items())))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--refresh", action="store_true", help="refresh the pinned snapshot from .cache")
    args = parser.parse_args()

    if args.refresh:
        project_snapshot()
    else:
        write_json(CATALOG_PATH, build_catalog())


if __name__ == "__main__":
    main()
