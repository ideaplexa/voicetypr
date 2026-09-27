import type { CloudModelInfo, SpeechModelEngine } from "@/types";
import { hasSttApiKey, removeSttApiKey, saveSttApiKey } from "@/utils/keyring";

export interface CloudProviderDefinition {
  id: string;
  engine: SpeechModelEngine;
  modelName: string;
  displayName: string;
  description: string;
  providerName: string;
  addKey: (key: string) => Promise<void>;
  removeKey: () => Promise<void>;
  hasKey: () => Promise<boolean>;
  docsUrl?: string;
  setupCta?: string;
}

const cloudProviderDefinitions = [
  {
    id: "soniox",
    displayName: "Soniox",
    providerName: "Soniox",
    description: "High-accuracy cloud transcription via Soniox APIs",
    docsUrl: "https://soniox.com/docs/stt/get-started",
  },
  {
    id: "openai",
    displayName: "OpenAI",
    providerName: "OpenAI",
    description: "Cloud transcription via OpenAI",
    docsUrl: "https://platform.openai.com/docs/guides/speech-to-text",
  },
  {
    id: "groq",
    displayName: "Groq",
    providerName: "Groq",
    description: "Fast cloud Whisper transcription via Groq",
    docsUrl: "https://console.groq.com/docs/speech-to-text",
  },
  {
    id: "deepgram",
    displayName: "Deepgram",
    providerName: "Deepgram",
    description: "Production cloud transcription via Deepgram Nova",
    docsUrl: "https://developers.deepgram.com/docs/pre-recorded-audio",
  },
  {
    id: "cohere",
    displayName: "Cohere",
    providerName: "Cohere",
    description: "Cloud transcription via Cohere Transcribe",
    docsUrl: "https://docs.cohere.com/docs/transcribe",
  },
] as const satisfies ReadonlyArray<
  Pick<CloudProviderDefinition, "id" | "displayName" | "providerName" | "description" | "docsUrl">
>;

export const CLOUD_PROVIDERS: Record<string, CloudProviderDefinition> = Object.fromEntries(
  cloudProviderDefinitions.map((definition) => [
    definition.id,
    {
      ...definition,
      engine: definition.id as SpeechModelEngine,
      modelName: definition.id,
      addKey: async (key: string) => {
        await saveSttApiKey(definition.id, key.trim());
      },
      removeKey: async () => {
        await removeSttApiKey(definition.id);
      },
      hasKey: async () => hasSttApiKey(definition.id),
      setupCta: "Add API Key",
    },
  ]),
);

export const getCloudProviderByModel = (modelName: string): CloudProviderDefinition | undefined =>
  CLOUD_PROVIDERS[modelName];

export const isCloudEngine = (engine: string): boolean =>
  Object.values(CLOUD_PROVIDERS).some((provider) => provider.engine === engine);

export function resolveCloudModelLabel(
  model: Pick<CloudModelInfo, "available_models" | "underlying_model">,
): string | undefined {
  const options = model.available_models ?? [];
  if (options.length === 0) return undefined;

  const selected = model.underlying_model
    ? options.find((option) => option.id === model.underlying_model)
    : undefined;

  return (selected ?? options[0])?.display_name;
}
