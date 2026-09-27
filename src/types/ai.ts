// AI Enhancement Types that match Rust structures

export type EnhancementPreset =
  | "PersonalDictation"
  | "CleanDictation"
  | "Writing"
  | "Notes"
  | "Message"
  | "Code";

export const AI_FORMATTING_PRESETS: EnhancementPreset[] = [
  "CleanDictation",
  "Writing",
  "Notes",
  "Message",
  "Code",
];

export const presetRequiresAiFormatting = (preset: EnhancementPreset): boolean =>
  preset !== "PersonalDictation";

/** UI label for a formatting preset. Persisted enum values stay unchanged. */
export const presetDisplayLabel = (preset: EnhancementPreset): string => {
  switch (preset) {
    case "PersonalDictation":
      return "Polish Off";
    case "CleanDictation":
      return "Clean Dictation";
    case "Writing":
      return "Writing";
    case "Notes":
      return "Notes";
    case "Message":
      return "Message";
    case "Code":
      return "Code";
  }
};

export const defaultPresetForAiEnabled = (aiEnabled: boolean): EnhancementPreset =>
  aiEnabled ? "CleanDictation" : "PersonalDictation";

/** Migrate a backend preset value (possibly legacy) to the V2 contract. */
export const migratePreset = (raw: string, aiEnabled = false): EnhancementPreset => {
  switch (raw) {
    case "PersonalDictation":
      return "PersonalDictation";
    case "CleanDictation":
      return "CleanDictation";
    case "Writing":
      return "Writing";
    case "Notes":
      return "Notes";
    case "Message":
      return "Message";
    case "Code":
      return "Code";
    case "Coding":
    case "Prompts":
    case "Commit":
      return "Code";
    case "Email":
      return "Writing";
    case "Default":
      return aiEnabled ? "CleanDictation" : "PersonalDictation";
    default:
      return "PersonalDictation";
  }
};

export interface EnhancementOptions {
  preset: EnhancementPreset;
}

export interface AISettings {
  enabled: boolean;
  provider: string;
  model: string;
  hasApiKey: boolean;
  modelsByProvider: Record<string, string>;
  reasoningByProvider: Record<string, string>;
  fastModeByProvider: Record<string, boolean>;
  aiModelNeedsReselection?: boolean;
}

export interface AIModel {
  id: string;
  name: string;
  provider: string;
  description?: string;
  reasoning?: boolean;
  contextWindow?: number | null;
  costInput?: number | null;
  costOutput?: number | null;
}

// Helper to convert between frontend camelCase and backend snake_case
export const toBackendOptions = (options: { preset: EnhancementPreset }): EnhancementOptions => ({
  preset: options.preset,
});

export const fromBackendOptions = (
  options: EnhancementOptions,
  aiEnabled = false,
): {
  preset: EnhancementPreset;
} => ({
  preset: migratePreset(options.preset as string, aiEnabled),
});
