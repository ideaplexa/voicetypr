#!/usr/bin/env python3
"""Generate the embedded AI provider catalog from the pinned models.dev snapshot."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent
SNAPSHOT_PATH = ROOT / "models.dev.snapshot.json"
OVERLAY_PATH = ROOT / "overlay.json"
CATALOG_PATH = ROOT / "catalog.generated.json"
FULL_API_PATH = ROOT / ".cache" / "models.dev.api.full.json"

STATUS_ORDER = {"production": 0, "experimental": 1, "hidden": 2}
PROVIDER_FIELDS = ("id", "name", "env", "npm", "doc", "api")
MODEL_FIELDS = (
    "id",
    "name",
    "family",
    "reasoning",
    "reasoning_options",
    "temperature",
    "status",
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


def eligible_model(model: dict[str, Any]) -> bool:
    modalities = model.get("modalities", {})
    model_id = str(model.get("id") or model.get("model_id", "")).lower()
    # Some specialist models are incorrectly labelled text->text upstream.
    non_chat_id = re.search(
        r"(?:^|[/_.-])(?:embedding\w*|embed|rerank\w*|tts|image|audio|moderation|whisper)(?:$|[/_.-])",
        model_id,
    )
    return (
        not non_chat_id
        and "text" in modalities.get("input", [])
        and modalities.get("output") == ["text"]
        and model.get("status") not in {"deprecated", "retired"}
    )


def project_model(model: dict[str, Any], recommended_ids: set[str]) -> dict[str, Any]:
    model_id = str(model.get("id") or model["model_id"])
    projected: dict[str, Any] = {
        "model_id": model_id,
        "label": str(model.get("name") or model.get("label") or humanize(model_id)),
        "recommended": model_id in recommended_ids,
        "reasoning": bool(model.get("reasoning", False)),
    }

    projected["temperature"] = bool(model.get("temperature", True))
    projected["reasoning_efforts"] = [
        value
        for option in model.get("reasoning_options", [])
        if option.get("type") == "effort"
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
        source_models = inline_models if inline_models is not None or snapshot_id is None else provider.get("models")
        models = [project_model(model, recommended_ids) for model in sorted_models(source_models)
                  if eligible_model(model)]
        missing = recommended_ids - {model["model_id"] for model in models}
        if missing:
            raise ValueError(f"{provider_id}: missing recommended models {sorted(missing)}")
        recommendation_order = {model_id: index for index, model_id in enumerate(config.get("recommended", []))}
        models.sort(key=lambda model: (recommendation_order.get(model["model_id"], len(recommendation_order)), model["model_id"]))
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
                "supports_reasoning": any(model["reasoning"] for model in models),
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
            if not eligible_model(model):
                continue
            if "model_ids" in config and model_id not in config["model_ids"]:
                continue
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
