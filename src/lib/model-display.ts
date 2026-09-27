import type { ModelInfo } from "@/types";

const KNOWN_MODEL_DISPLAY_NAMES: Record<string, string> = {
  "base.en": "Base (English)",
  "small.en": "Small (English)",
  "large-v3": "Large v3",
  "large-v3-turbo": "Large v3 Turbo",
  "large-v3-turbo-q8_0": "Large v3 Turbo (Q8)",
  "parakeet-tdt-0.6b-v3": "Parakeet V3",
  "parakeet-tdt-0.6b-v2": "Parakeet V2 (English)",
  soniox: "Soniox (Cloud)",
  openai: "OpenAI (Cloud)",
  groq: "Groq (Cloud)",
  deepgram: "Deepgram (Cloud)",
  cohere: "Cohere (Cloud)",
};

function titleCaseToken(token: string) {
  if (!token) return token;
  if (/^v\d+$/i.test(token)) return token.toLowerCase();
  if (/^q\d+_\d+$/i.test(token)) return token.toUpperCase();
  if (/^\d+(\.\d+)?b$/i.test(token)) return token.toUpperCase();

  return token.charAt(0).toUpperCase() + token.slice(1);
}

export function humanizeModelId(modelId: string) {
  return modelId
    .replace(/\.en$/i, " English")
    .replace(/[_-]+/g, " ")
    .split(" ")
    .filter(Boolean)
    .map(titleCaseToken)
    .join(" ");
}

export function getModelDisplayName(
  modelId: string | null | undefined,
  models?: Record<string, ModelInfo>,
) {
  if (!modelId) return null;

  return (
    models?.[modelId]?.display_name ||
    KNOWN_MODEL_DISPLAY_NAMES[modelId] ||
    humanizeModelId(modelId)
  );
}
