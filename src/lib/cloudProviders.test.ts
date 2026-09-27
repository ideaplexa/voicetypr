import { describe, expect, it } from "vitest";
import { resolveCloudModelLabel } from "./cloudProviders";

describe("resolveCloudModelLabel", () => {
  const availableModels = [
    { id: "gpt-transcribe", display_name: "GPT Transcribe" },
    { id: "gpt-4o-mini-transcribe", display_name: "GPT-4o mini Transcribe" },
  ];

  it("returns the friendly label for the selected API model", () => {
    expect(
      resolveCloudModelLabel({
        underlying_model: "gpt-4o-mini-transcribe",
        available_models: availableModels,
      }),
    ).toBe("GPT-4o mini Transcribe");
  });

  it("falls back to the curated default instead of exposing an unknown API id", () => {
    expect(
      resolveCloudModelLabel({
        underlying_model: "unknown-api-model",
        available_models: availableModels,
      }),
    ).toBe("GPT Transcribe");
  });

  it("returns undefined when the backend provides no curated options", () => {
    expect(
      resolveCloudModelLabel({
        underlying_model: "raw-api-id",
        available_models: [],
      }),
    ).toBeUndefined();
  });
});
