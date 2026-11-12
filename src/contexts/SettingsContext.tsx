import { recordEvent } from "@/lib/observability";
import {
  createContext,
  useContext,
  ReactNode,
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { AppSettings } from "@/types";
import { createLogger } from "@/lib/logger";

const log = createLogger("settings");

interface SettingsContextType {
  settings: AppSettings | null;
  isLoading: boolean;
  error: Error | null;
  refreshSettings: () => Promise<void>;
  updateSettings: (
    updates: Partial<AppSettings>,
    options?: { publishAfterSave?: boolean },
  ) => Promise<void>;
}

const SettingsContext = createContext<SettingsContextType | null>(null);

function settingsEqual(a: AppSettings | null, b: AppSettings): boolean {
  if (!a) return false;
  const left = a as unknown as Record<string, unknown>;
  const right = b as unknown as Record<string, unknown>;
  const keys = Object.keys(left);
  if (keys.length !== Object.keys(right).length) return false;
  return keys.every((key) => Object.is(left[key], right[key]) || isEqualJson(left[key], right[key]));
}

function isEqualJson(a: unknown, b: unknown): boolean {
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  // Always-current mirror of `settings`, kept in sync at every write site. Lets
  // updateSettings read the latest state without closing over a stale snapshot.
  const settingsRef = useRef<AppSettings | null>(null);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());
  const revision = useRef(0);
  const keyRevisions = useRef<Partial<Record<keyof AppSettings, number>>>({});

  const loadSettings = useCallback(async () => {
    try {
      let pending: Promise<void>;
      do {
        pending = saveQueue.current;
        await pending;
      } while (pending !== saveQueue.current);
      const loadRevision = revision.current;
      const appSettings = await invoke<AppSettings>("get_settings");
      if (loadRevision !== revision.current) return;
      // Refreshes keep the previous object (and every consumer's render) when
      // nothing changed; `isLoading` only ever covers the first load.
      if (!settingsEqual(settingsRef.current, appSettings)) {
        setSettings(appSettings);
        settingsRef.current = appSettings;
      }
      setError((current) => (current === null ? current : null));
    } catch (err) {
      const error = err instanceof Error ? err : new Error("Failed to load settings");
      setError(error);
      log.error("[SettingsContext] Failed to load settings:", error);
    } finally {
      setIsLoading((current) => (current ? false : current));
    }
  }, []);

  const updateSettings = useCallback(
    async (updates: Partial<AppSettings>, options?: { publishAfterSave?: boolean }) => {
      // Merge optimistic edits immediately, but send full snapshots serially.
      const prev = settingsRef.current;
      if (!prev) {
        return;
      }

      const nextSettings = { ...prev, ...updates };
      const updateRevision = ++revision.current;
      const keys = Object.keys(updates) as (keyof AppSettings)[];
      for (const key of keys) keyRevisions.current[key] = updateRevision;
      const publishAfterSave = options?.publishAfterSave === true;
      if (!publishAfterSave) {
        settingsRef.current = nextSettings;
        // Optimistic update - update state immediately so UI responds instantly.
        setSettings(nextSettings);
      }

      const save = saveQueue.current.then(async () => {
        const current = settingsRef.current;
        if (!current) return;
        const snapshot = { ...current };
        if (publishAfterSave) {
          const values = snapshot as unknown as Record<string, unknown>;
          const updateValues = updates as Record<string, unknown>;
          for (const key of keys) {
            if (keyRevisions.current[key] === updateRevision) values[key] = updateValues[key];
          }
        }

        try {
          await invoke("save_settings", {
            settings: snapshot,
            ...(Object.prototype.hasOwnProperty.call(updates, "update_channel")
              ? { updateChannelExplicit: true }
              : {}),
          });

          const changed: ["mode" | "live_preview" | "language" | "engine" | "mic", keyof AppSettings][] = [
            ["engine", "current_model"], ["mic", "selected_microphone"],
            ["mode", "recording_mode"], ["live_preview", "transcription_mode"], ["language", "speech_language"],
          ];
          for (const [setting, key] of changed) {
            if (key in updates && updates[key] !== prev[key]) {
              recordEvent({ name: "quick_setting_changed", properties: { setting, source: "app" } });
            }
          }

          if (publishAfterSave && settingsRef.current) {
            // Publish only this action's still-current keys after durable save.
            const published = { ...settingsRef.current } as unknown as Record<string, unknown>;
            const updateValues = updates as Record<string, unknown>;
            for (const key of keys) {
              if (keyRevisions.current[key] === updateRevision) published[key] = updateValues[key];
            }
            settingsRef.current = published as unknown as AppSettings;
            setSettings(settingsRef.current);
          }
        } catch (err) {
          if (!publishAfterSave && settingsRef.current) {
            const reverted = { ...settingsRef.current } as unknown as Record<string, unknown>;
            const prevValues = prev as unknown as Record<string, unknown>;
            for (const key of keys) {
              if (keyRevisions.current[key] === updateRevision) reverted[key] = prevValues[key];
            }
            settingsRef.current = reverted as unknown as AppSettings;
            setSettings(settingsRef.current);
          }
          log.error("[SettingsContext] Failed to update settings:", err);
          throw err;
        }
      });
      // A rejected save must not prevent later actions from being saved.
      saveQueue.current = save.catch(() => undefined);
      await save;
    },
    [],
  );

  // Load settings on mount
  useEffect(() => {
    void (async () => {
      await Promise.resolve();
      await loadSettings();
    })();
  }, [loadSettings]);

  // Listen for settings changes from other sources (e.g., tray menu or backend auto-selection)
  useTauriEvent("model-changed", () => void loadSettings());
  useTauriEvent("language-changed", () => void loadSettings());
  useTauriEvent("audio-device-changed", () => void loadSettings());
  useTauriEvent("settings-changed", () => void loadSettings());

  const value = useMemo(
    () => ({
      settings,
      isLoading,
      error,
      refreshSettings: loadSettings,
      updateSettings,
    }),
    [settings, isLoading, error, loadSettings, updateSettings],
  );

  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings() {
  const context = useContext(SettingsContext);
  if (!context) {
    throw new Error("useSettings must be used within a SettingsProvider");
  }
  return context;
}

// Helper hook for components that only need specific settings
export function useSetting<K extends keyof AppSettings>(key: K): AppSettings[K] | undefined {
  const { settings } = useSettings();
  return settings?.[key];
}
