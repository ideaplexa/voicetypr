export interface PasteOutcomePayload {
  generation?: number;
  outcome: "pasted" | "copied" | "no_permission";
  words: number;
  polished: boolean;
}
