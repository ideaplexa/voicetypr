import { expect, it } from "vitest";
import { recordingAudioUpdate, recordingAudioValue } from "@/lib/recording-audio-setting";
import type { AppSettings } from "@/types";

const base: AppSettings = { hotkey: "", current_model: "", speech_language: "en", theme: "system" };

it("maps the new default and explicit Don't save separately, without changing retention", () => {
  expect(recordingAudioValue(base)).toBe("failed");
  expect(recordingAudioValue({ ...base, save_recordings: false, save_failed_recordings: false })).toBe("off");
  expect(recordingAudioUpdate("failed")).toEqual({ save_recordings: false, save_failed_recordings: true });
  expect(recordingAudioUpdate("off")).toEqual({ save_recordings: false, save_failed_recordings: false });
});

it("preserves all existing keep periods including forever", () => {
  for (const days of [7, 14, 30, 90]) {
    const update = recordingAudioUpdate(String(days));
    expect(update).toEqual({ save_recordings: true, recording_retention_days: days });
    expect(recordingAudioValue({ ...base, ...update })).toBe(String(days));
  }
  const update = recordingAudioUpdate("forever");
  expect(update).toEqual({ save_recordings: true, recording_retention_days: null });
  expect(recordingAudioValue({ ...base, ...update })).toBe("forever");
});
