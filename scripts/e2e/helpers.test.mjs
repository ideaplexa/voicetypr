import test from "node:test";
import assert from "node:assert/strict";
import { PARAKEET_CACHE_SUBDIRS, compareReports, parseArgs, profilePaths, seedSettings, wer } from "./helpers.mjs";

test("parseArgs accepts flags and values, rejects missing and unknown options", () => {
  assert.deepEqual(parseArgs(["--check-setup", "--clips", "/tmp/clips", "--cases", "regular,live-preview"]), {
    "check-setup": true, clips: "/tmp/clips", cases: "regular,live-preview",
  });
  assert.throws(() => parseArgs(["--clips"]), /needs a value/u);
  assert.throws(() => parseArgs(["--bogus"]), /Unknown option/u);
});

test("WER is case and punctuation insensitive and uses word edits", () => {
  assert.equal(wer("Hello, brave world!", "hello brave world"), 0);
  assert.equal(wer("one two three", "one four three"), 1 / 3);
  assert.equal(wer("one two", "one"), 0.5);
  assert.equal(wer("", ""), 0);
  assert.equal(wer("", "extra"), 1);
});

test("settings seed matches the isolated Tauri store and selected mode", () => {
  const paths = profilePaths("/tmp/e2e-home");
  assert.equal(paths.settings, "/tmp/e2e-home/Library/Application Support/com.ideaplexa.voicetypr/settings");
  assert.equal(paths.logs, "/tmp/e2e-home/Library/Logs/com.ideaplexa.voicetypr");
  const settings = seedSettings({ engine: "parakeet", model: "parakeet-tdt-0.6b-v3", livePreview: true });
  assert.equal(settings.onboarding_completed, true);
  assert.equal(settings.selected_microphone, "BlackHole 2ch");
  assert.equal(settings.recording_mode, "toggle");
  assert.equal(settings.transcription_mode, "live_preview");
  assert.equal(settings.current_model_engine, "parakeet");
  assert.equal(settings.play_sound_on_recording, false);
  assert.equal(seedSettings({ engine: "whisper", model: "base.en", livePreview: false }).transcription_mode, "regular");
  const cloud = seedSettings({ engine: "soniox", model: "stt-async-v5", livePreview: true });
  assert.equal(cloud.current_model, "soniox");
  assert.deepEqual(cloud.cloud_stt_models_by_provider, { soniox: "stt-async-v5" });
  assert.equal(PARAKEET_CACHE_SUBDIRS["parakeet-unified-640ms"], "parakeet-unified-en-0.6b");
  assert.equal(PARAKEET_CACHE_SUBDIRS["nemotron-multilingual-1120ms"], "nemotron-multilingual/multilingual/1120ms");
});

test("report comparison uses absolute WER and stop-to-text thresholds", () => {
  const baseline = { cases: [{ id: "a", wer: 0.1, stop_to_text_ms: 500 }] };
  assert.deepEqual(compareReports({ cases: [{ id: "a", wer: 0.15, stop_to_text_ms: 750 }] }, baseline, { werThreshold: 0.05, latencyThresholdMs: 250 }), []);
  const differences = compareReports({ cases: [{ id: "a", wer: 0.16, stop_to_text_ms: 751 }] }, baseline, { werThreshold: 0.05, latencyThresholdMs: 250 });
  assert.equal(differences.length, 2);
  assert.throws(() => compareReports({ cases: [{ id: "missing" }] }, baseline, { werThreshold: 0, latencyThresholdMs: 0 }), /Baseline lacks case/u);
});
