import test from "node:test";
import assert from "node:assert/strict";
import { PARAKEET_CACHE_SUBDIRS, compareReports, parseArgs, parsePerfLines, perfColumns, profilePaths, seedSettings, soakGrowth, wer } from "./helpers.mjs";

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
  const cloud = seedSettings({ engine: "soniox", model: "stt-async-v6", livePreview: true });
  assert.equal(cloud.current_model, "soniox");
  assert.deepEqual(cloud.cloud_stt_models_by_provider, { soniox: "stt-async-v6" });
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

test("parseArgs accepts --perf and --soak and leaves them absent by default", () => {
  assert.deepEqual(parseArgs(["--perf", "--soak", "30"]), { perf: true, soak: "30" });
  assert.equal(parseArgs(["--clips", "x"]).perf, undefined);
  assert.throws(() => parseArgs(["--soak"]), /needs a value/u);
});

const PERF_LOG = [
  "2026-10-04 INFO unrelated line",
  "2026-10-04T10:00:00 INFO voicetypr::perf: [PERF] take=7 hotkey_ms=1000 starting_ms=1004 recording_ms=1120 pill_show_called_ms=1121 pill_shown_ms=1150",
  "2026-10-04T10:00:00 INFO voicetypr::perf: [PERF] take=7 island_visible_ms=1180",
  "2026-10-04T10:00:05 INFO voicetypr::perf: [PERF] take=7 rss_mb=312.4",
  "2026-10-04T10:00:05 INFO voicetypr::perf: [PERF] take=7 frames=600 p50_ms=16.7 p95_ms=18.0 max_ms=55.0 over16=12 over33=2 over50=1 level_events=590 level_gap_p95_ms=20.0 pill_dom_nodes=210 stack_items=0 longtasks=-",
];

test("[PERF] parser merges the lines of one take and maps '-' to null", () => {
  const [take, ...rest] = parsePerfLines(PERF_LOG);
  assert.equal(rest.length, 0);
  assert.equal(take.take, 7);
  assert.equal(take.hotkey_ms, 1000);
  assert.equal(take.island_visible_ms, 1180);
  assert.equal(take.rss_mb, 312.4);
  assert.equal(take.p95_ms, 18);
  assert.equal(take.longtasks, null);
  assert.deepEqual(parsePerfLines(["nothing here", "[PERF] take=x a=1"]), []);
});

test("perf report columns derive hotkey→island and pick the last take with pill numbers", () => {
  const columns = perfColumns(PERF_LOG);
  assert.equal(columns.hotkey_to_island_ms, 180);
  assert.equal(columns.hotkey_to_recording_ms, 120);
  assert.equal(columns.frame_p95_ms, 18);
  assert.equal(columns.frame_max_ms, 55);
  assert.equal(columns.frame_over33, 2);
  assert.equal(columns.level_gap_p95_ms, 20);
  assert.equal(columns.pill_dom_nodes, 210);
  assert.equal(columns.rss_mb, 312.4);
  assert.equal(perfColumns([]).hotkey_to_island_ms, null);
});

test("island metric keeps hidden-start and already-visible takes apart", () => {
  const hidden = perfColumns(["[PERF] take=1 hotkey_ms=1000 recording_ms=1100", "[PERF] take=1 island_visible_ms=1040"]);
  assert.equal(hidden.hotkey_to_island_ms, 40);
  assert.equal(hidden.hotkey_to_island_already_visible_ms, null);
  const warm = perfColumns(["[PERF] take=2 hotkey_ms=2000 recording_ms=2100", "[PERF] take=2 island_already_visible_ms=2016"]);
  assert.equal(warm.hotkey_to_island_ms, null);
  assert.equal(warm.hotkey_to_island_already_visible_ms, 16);
  assert.equal(perfColumns([]).hotkey_to_island_already_visible_ms, null);
});

test("[PERF] parser keeps show_mode as text and exposes it as a report column", () => {
  const lines = ["[PERF] take=1 hotkey_ms=1000 starting_ms=1004 recording_ms=1100 pill_show_called_ms=1005 pill_shown_ms=1030 show_mode=alpha", "[PERF] take=1 island_visible_ms=1040"];
  assert.equal(parsePerfLines(lines)[0].show_mode, "alpha");
  assert.equal(perfColumns(lines).show_mode, "alpha");
  assert.equal(perfColumns(PERF_LOG).show_mode, null);
  assert.equal(perfColumns([]).show_mode, null);
});

test("soak growth flags DOM growth and large RSS growth, and ignores flat runs", () => {
  const make = (dom, rss) => ({ case: "regular", engine: "parakeet", model: "m", language: "en", mode: "regular", pill_dom_nodes: dom, rss_mb: rss });
  const [flat] = soakGrowth([make(200, 300), make(200, 305), make(200, 310)]);
  assert.equal(flat.growth, false);
  assert.equal(flat.takes, 3);
  const [grown] = soakGrowth([make(200, 300), make(260, 340)]);
  assert.equal(grown.growth, true);
  assert.equal(grown.dom_delta, 60);
  assert.equal(grown.flags.length, 2);
  assert.deepEqual(soakGrowth([make(200, 300)]), []);
  assert.deepEqual(soakGrowth([{ case: "cancel", engine: "e", model: "m", language: "en", mode: "regular" }]), []);
});
