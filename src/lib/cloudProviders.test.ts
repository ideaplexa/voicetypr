import { describe, expect, it } from "vitest";
import { resolveCloudModelLabel } from "./cloudProviders";

describe("resolveCloudModelLabel", () => {
  const availableModels = [
    { id: "whisper-large-v3-turbo", display_name: "Whisper Large v3 Turbo" },
    { id: "whisper-large-v3", display_name: "Whisper Large v3" },
  ];

  it("returns the friendly label for the selected API model", () => {
    expect(
      resolveCloudModelLabel({
        underlying_model: "whisper-large-v3",
        available_models: availableModels,
      }),
    ).toBe("Whisper Large v3");
  });

  it("falls back to the curated default instead of exposing an unknown API id", () => {
    expect(
      resolveCloudModelLabel({
        underlying_model: "unknown-api-model",
        available_models: availableModels,
      }),
    ).toBe("Whisper Large v3 Turbo");
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
