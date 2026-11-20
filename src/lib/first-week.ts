import type { FirstWeekProgress } from "@/types";

/** One card per day of the first week, in order of value (MOI-154 map). Day N shows FIRST_WEEK_CARDS[N - 1]. */
export const FIRST_WEEK_CARDS = ["hold", "dictionary", "polish", "voice_commands", "realtime", "file", "share"] as const;
export type FirstWeekCardId = (typeof FIRST_WEEK_CARDS)[number];
export const FIRST_WEEK_LENGTH = FIRST_WEEK_CARDS.length;
const FINISHED_DAY = FIRST_WEEK_LENGTH + 1;

/** What Home knows about this person right now; null means not loaded yet, so no card relies on it. */
export interface FirstWeekState {
  /** Dictation is allowed by the licence or trial. */
  licenseValid: boolean | null;
  holdToTalk: boolean | null;
  dictionaryEmpty: boolean | null;
  polishOn: boolean | null;
  voiceCommandsOn: boolean | null;
  /** The selected transcription source is ready to transcribe. */
  sourceReady: boolean;
  /** The selected source supports Real-time words. */
  streams: boolean | null;
  realtimeOn: boolean;
  /** A downloaded on-device model is selected here and this computer isn't using another one. */
  canShareThisComputer: boolean | null;
  sharingOn: boolean | null;
}

export function localDate(now: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** Days count the local dates Home was opened on, so nobody misses a card by being away. Null when nothing changes. */
export function advanceFirstWeek(progress: FirstWeekProgress | undefined, today: string): FirstWeekProgress | null {
  const current = progress ?? { day: 0, last_seen: null, dismissed: [] };
  if (current.day >= FINISHED_DAY) return null;
  if (current.day > 0 && current.last_seen !== null && today <= current.last_seen) return null;
  return { ...current, day: current.day + 1, last_seen: today };
}

export function retireFirstWeekCard(progress: FirstWeekProgress, id: FirstWeekCardId): FirstWeekProgress {
  return progress.dismissed.includes(id) ? progress : { ...progress, dismissed: [...progress.dismissed, id] };
}

/** A card shows only once its feature can work for this person and isn't already in use. */
function canShow(id: FirstWeekCardId, state: FirstWeekState): boolean {
  switch (id) {
    case "hold":
      return state.holdToTalk === false;
    case "dictionary":
      return state.dictionaryEmpty === true;
    case "polish":
      return state.polishOn === false;
    case "voice_commands":
      return state.voiceCommandsOn === false;
    case "realtime":
      return state.streams === true && !state.realtimeOn;
    case "file":
      return state.sourceReady;
    case "share":
      return state.canShareThisComputer === true && state.sharingOn === false;
  }
}

/** Exactly today's card: one a day. A card that can't work today, or was put away, leaves the day empty. */
export function firstWeekCard(progress: FirstWeekProgress | undefined, state: FirstWeekState): FirstWeekCardId | null {
  if (state.licenseValid !== true || !progress) return null;
  if (progress.day < 1 || progress.day >= FINISHED_DAY) return null;
  const id = FIRST_WEEK_CARDS[progress.day - 1];
  return !progress.dismissed.includes(id) && canShow(id, state) ? id : null;
}

/** The backend always sends first_week; without it (an older backend) Home keeps its usual tips. */
export function firstWeekActive(progress: FirstWeekProgress | undefined): boolean {
  return progress !== undefined && progress.day < FINISHED_DAY;
}
