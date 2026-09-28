import { join } from "node:path";

export const APP_ID = "com.ideaplexa.voicetypr";
export const HOTKEY = "Control+Alt+F9";
export const CASES = ["regular", "live-preview", "first-word", "instant-speech", "last-word", "back-to-back", "cancel", "no-speech"];
export const PARAKEET_CACHE_SUBDIRS = {
  "parakeet-tdt-0.6b-v3": "parakeet-tdt-0.6b-v3",
  "parakeet-tdt-0.6b-v2": "parakeet-tdt-0.6b-v2",
  "parakeet-unified-640ms": "parakeet-unified-en-0.6b",
  "nemotron-multilingual-1120ms": "nemotron-multilingual/multilingual/1120ms",
};

export function parseArgs(argv) {
  const flags = new Set(["help", "check-setup"]);
  const values = new Set(["clips", "bin", "engines", "engine", "model", "cases", "baseline", "tail-ms", "wer-threshold", "latency-threshold-ms", "timeout-ms", "secure-store"]);
  const result = {};
  for (let i = 0; i < argv.length; i += 1) {
    const word = argv[i];
    if (!word.startsWith("--")) throw new Error(`Unexpected argument: ${word}`);
    const key = word.slice(2);
    if (flags.has(key)) {
      result[key] = true;
    } else if (values.has(key)) {
      const value = argv[++i];
      if (!value || value.startsWith("--")) throw new Error(`--${key} needs a value`);
      result[key] = value;
    } else {
      throw new Error(`Unknown option: ${word}`);
    }
  }
  return result;
}

export function csv(value) {
  return value.split(",").map((part) => part.trim()).filter(Boolean);
}

export function positiveNumber(value, name, allowZero = false) {
  const number = Number(value);
  if (!Number.isFinite(number) || (allowZero ? number < 0 : number <= 0)) {
    throw new Error(`${name} must be ${allowZero ? "non-negative" : "positive"}`);
  }
  return number;
}

export function profilePaths(home) {
  return {
    data: join(home, "Library", "Application Support", APP_ID),
    settings: join(home, "Library", "Application Support", APP_ID, "settings"),
    logs: join(home, "Library", "Logs", APP_ID),
    whisperModels: join(home, "Library", "Application Support", APP_ID, "models"),
    fluidModels: join(home, "Library", "Application Support", "FluidAudio", "Models"),
  };
}

export function seedSettings({ engine, model, livePreview, language = "en" }) {
  if (!engine || !model) throw new Error("Engine and model are required to seed settings");
  const cloud = !new Set(["whisper", "parakeet"]).has(engine);
  return {
    onboarding_completed: true,
    hotkey: HOTKEY,
    recording_mode: "toggle",
    selected_microphone: "BlackHole 2ch",
    current_model: cloud ? engine : model,
    current_model_engine: engine,
    ...(cloud ? { cloud_stt_models_by_provider: { [engine]: model } } : {}),
    speech_language: language,
    transcription_mode: livePreview ? "live_preview" : "regular",
    play_sound_on_recording: false,
    play_sound_on_transcription_complete: false,
    play_sound_on_paste_success: false,
    pause_media_during_recording: false,
    auto_paste_transcription: true,
    pill_indicator_mode: "when_recording",
  };
}

export function words(value) {
  return value.toLowerCase().normalize("NFKD").replace(/[\p{P}\p{S}]+/gu, " ").split(/\s+/u).filter(Boolean);
}

export function wer(reference, hypothesis) {
  const a = words(reference);
  const b = words(hypothesis);
  if (!a.length) return b.length ? 1 : 0;
  let previous = Array.from({ length: b.length + 1 }, (_, index) => index);
  for (let i = 1; i <= a.length; i += 1) {
    const row = [i];
    for (let j = 1; j <= b.length; j += 1) {
      row[j] = Math.min(previous[j] + 1, row[j - 1] + 1, previous[j - 1] + Number(a[i - 1] !== b[j - 1]));
    }
    previous = row;
  }
  return previous[b.length] / a.length;
}

export function compareReports(current, baseline, { werThreshold, latencyThresholdMs }) {
  const previous = new Map(baseline.cases.map((item) => [item.id, item]));
  const regressions = [];
  for (const item of current.cases) {
    const prior = previous.get(item.id);
    if (!prior) throw new Error(`Baseline lacks case ${item.id}`);
    if (item.wer != null && prior.wer != null && item.wer - prior.wer > werThreshold) {
      regressions.push(`${item.id}: WER ${prior.wer.toFixed(3)} → ${item.wer.toFixed(3)}`);
    }
    if (item.stop_to_text_ms != null && prior.stop_to_text_ms != null && item.stop_to_text_ms - prior.stop_to_text_ms > latencyThresholdMs) {
      regressions.push(`${item.id}: stop→text ${prior.stop_to_text_ms} → ${item.stop_to_text_ms} ms`);
    }
  }
  return regressions;
}
