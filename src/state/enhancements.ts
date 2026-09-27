import { create } from "zustand";

export type PolishErrorKind = "auth" | "generic";

export type PolishError = {
  kind: PolishErrorKind;
  message: string;
};

type EnhancementsState = {
  polishError: PolishError | null;
  setPolishError: (kind: PolishErrorKind, message: string) => void;
  clearPolishError: () => void;
};

export const useEnhancementsStore = create<EnhancementsState>((set) => ({
  polishError: null,
  setPolishError: (kind, message) => set({ polishError: { kind, message } }),
  clearPolishError: () => set({ polishError: null }),
}));
