import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { create } from "zustand";
import { createLogger } from "@/lib/logger";
import { defaultWritingSettings, mergeWritingSettings, type WritingSettings } from "@/types/writing";
import { getErrorMessage } from "@/utils/error";

const log = createLogger("enhancements");

type WritingSettingsState = {
  settings: WritingSettings;
  loaded: boolean;
  load: (signal?: AbortSignal) => Promise<boolean>;
  update: (patch: Partial<WritingSettings>) => void;
};

export const useWritingSettings = create<WritingSettingsState>((set, get) => {
  let saveQueue: Promise<void> = Promise.resolve();
  let saveGeneration = 0;
  let lastPersisted = defaultWritingSettings;
  let inFlightLoad: Promise<boolean> | null = null;
  let pendingPatch: Partial<WritingSettings> = {};

  const enqueueSave = (next: WritingSettings, generation: number) => {
    saveQueue = saveQueue.then(async () => {
      try {
        await invoke("update_writing_settings", { settings: next });
        lastPersisted = next;
      } catch (error) {
        if (saveGeneration === generation) {
          set({ settings: lastPersisted });
          toast.error(getErrorMessage(error, "Failed to save writing settings"));
        }
      }
    });
  };

  return {
    settings: defaultWritingSettings,
    loaded: false,
    load: (signal) => {
      if (signal?.aborted) return Promise.resolve(false);
      if (get().loaded) return Promise.resolve(true);
      if (!inFlightLoad) {
        const request = (async () => {
          try {
            const response = await invoke<Partial<WritingSettings>>("get_writing_settings");
            const previous = mergeWritingSettings(response);
            lastPersisted = previous;
            const next = { ...previous, ...pendingPatch };
            const hasPendingChanges = Object.keys(pendingPatch).length > 0;
            pendingPatch = {};
            set({ settings: next, loaded: true });
            if (hasPendingChanges) enqueueSave(next, ++saveGeneration);
            return true;
          } catch (error) {
            log.error("Failed to load writing settings:", error);
            return false;
          }
        })();
        inFlightLoad = request;
        void request.finally(() => {
          if (inFlightLoad === request) inFlightLoad = null;
        });
      }
      return inFlightLoad.then((loaded) => loaded && !signal?.aborted);
    },
    update: (patch) => {
      const { settings, loaded } = get();
      const next = { ...settings, ...patch };
      set({ settings: next });
      if (loaded) {
        enqueueSave(next, ++saveGeneration);
      } else {
        pendingPatch = { ...pendingPatch, ...patch };
      }
    },
  };
});
