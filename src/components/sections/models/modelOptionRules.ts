import type { SpeechModelEngine } from "@/types";

/** Pause-based paragraphs need engine timings: Whisper, the Parakeet TDT models, Deepgram. */
export function supportsParagraphs(engine: SpeechModelEngine, modelName: string): boolean {
  if (engine === "whisper" || engine === "deepgram") return true;
  if (engine === "parakeet") return /tdt/i.test(modelName);
  return false;
}

/** Filler removal is an English-only rule. */
export function supportsFillerRemoval(languageValue: string, isEnglishOnlyModel: boolean): boolean {
  return isEnglishOnlyModel || /^en(-|_|$)/i.test(languageValue);
}

/** The island shows words only while streaming is on and its own switch allows it. */
export function islandLiveWordsActive(streams: boolean, realtimeOn: boolean): boolean {
  return streams && realtimeOn;
}
