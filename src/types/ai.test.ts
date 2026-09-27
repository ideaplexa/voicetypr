import { describe, expect, it } from "vitest";

import {
  defaultPresetForAiEnabled,
  fromBackendOptions,
  migratePreset,
  presetDisplayLabel,
  presetRequiresAiFormatting,
  type EnhancementPreset,
} from "./ai";
import parityFixture from "../../tests/fixtures/preset-parity.json";

const CURRENT_MODES: EnhancementPreset[] = [
  "PersonalDictation",
  "CleanDictation",
  "Writing",
  "Notes",
  "Message",
  "Code",
];

describe("AI enhancement mode migration", () => {
  it("preserves current mode values", () => {
    for (const mode of CURRENT_MODES) {
      expect(migratePreset(mode)).toBe(mode);
    }
  });

  it("maps legacy modes to the V2 contract", () => {
    expect(migratePreset("Default", false)).toBe("PersonalDictation");
    expect(migratePreset("Default", true)).toBe("CleanDictation");
    expect(migratePreset("Prompts")).toBe("Code");
    expect(migratePreset("Email")).toBe("Writing");
    expect(migratePreset("Commit")).toBe("Code");
    expect(migratePreset("Coding")).toBe("Code");
  });

  it("falls back to PersonalDictation for unknown modes", () => {
    expect(migratePreset("CustomLegacyPreset")).toBe("PersonalDictation");
  });

  it("identifies AI formatting requirements", () => {
    expect(presetRequiresAiFormatting("PersonalDictation")).toBe(false);
    expect(presetRequiresAiFormatting("CleanDictation")).toBe(true);
    expect(presetRequiresAiFormatting("Code")).toBe(true);
  });

  it("selects defaults based on AI formatting state", () => {
    expect(defaultPresetForAiEnabled(false)).toBe("PersonalDictation");
    expect(defaultPresetForAiEnabled(true)).toBe("CleanDictation");
  });

  it("labels the non-AI app rule as Polish Off", () => {
    expect(presetDisplayLabel("PersonalDictation")).toBe("Polish Off");
  });

  it("migrates backend options without changing current modes", () => {
    expect(
      fromBackendOptions({
        preset: "Message",
      }),
    ).toEqual({
      preset: "Message",
    });
  });

  it("migrates legacy backend options using AI context", () => {
    expect(
      fromBackendOptions(
        {
          preset: "Default" as EnhancementPreset,
        },
        true,
      ),
    ).toEqual({
      preset: "CleanDictation",
    });
  });

  it("matches the shared preset parity fixture", () => {
    for (const testCase of parityFixture.migrations) {
      expect(migratePreset(testCase.raw, testCase.aiEnabled)).toBe(testCase.expected);
    }

    for (const testCase of parityFixture.requiresAiFormatting) {
      expect(presetRequiresAiFormatting(testCase.preset as EnhancementPreset)).toBe(
        testCase.expected,
      );
    }

    for (const testCase of parityFixture.defaults) {
      expect(defaultPresetForAiEnabled(testCase.aiEnabled)).toBe(testCase.expected);
    }
  });
});
