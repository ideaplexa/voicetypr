export interface PasteOutcomePayload {
  outcome: "pasted" | "copied" | "no_permission";
  words: number;
}
