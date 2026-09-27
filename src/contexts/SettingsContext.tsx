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

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  // Always-current mirror of `settings`, kept in sync at every write site. Lets
  // updateSettings read the latest state without closing over a stale snapshot.
  const settingsRef = useRef<AppSettings | null>(null);

  const loadSettings = useCallback(async () => {
    try {
      setIsLoading(true);
      setError(null);
      const appSettings = await invoke<AppSettings>("get_settings");
      setSettings(appSettings);
      settingsRef.current = appSettings;
    } catch (err) {
      const error = err instanceof Error ? err : new Error("Failed to load settings");
      setError(error);
      log.error("[SettingsContext] Failed to load settings:", error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  const updateSettings = useCallback(
    async (updates: Partial<AppSettings>, options?: { publishAfterSave?: boolean }) => {
      // Read the latest settings from the ref rather than the `settings` captured
      // by this closure. The old `[settings]` dependency captured a snapshot at
      // closure-creation time, so an in-flight call held a stale `previousSettings`
      // and its failure-rollback could discard a concurrent update that landed in
      // between (and a concurrent call would even compute its next state from that
      // same stale base, overwriting the first call's optimistic write).
      const prev = settingsRef.current;
      if (!prev) {
        return;
      }

      const nextSettings = { ...prev, ...updates };
      const publishAfterSave = options?.publishAfterSave === true;
      if (!publishAfterSave) {
        settingsRef.current = nextSettings;
        // Optimistic update - update state immediately so UI responds instantly.
        setSettings(nextSettings);
      }

      try {
        await invoke("save_settings", {
          settings: nextSettings,
          ...(Object.prototype.hasOwnProperty.call(updates, "update_channel")
            ? { updateChannelExplicit: true }
            : {}),
        });

        if (publishAfterSave) {
          // Onboarding completion controls which application tree is mounted.
          // Publish it only after the backend confirms the durable save.
          settingsRef.current = nextSettings;
          setSettings(nextSettings);
        }
      } catch (err) {
        if (publishAfterSave) {
          log.error("[SettingsContext] Failed to update settings:", err);
          throw err;
        }

        // Roll back only the keys this call touched, preserving any concurrent
        // updates applied since (to other keys). This is the surgical revert the
        // stale-closure version got wrong when it restored the whole snapshot.
        // Spread the latest state, then restore only this call's touched keys to
        // their pre-call values. Casts sidestep TS's union-index write narrowing
        // (it would otherwise collapse the write target to `never`); the restored
        // values come from `prev`, the snapshot taken before applying `updates`.
        const reverted = { ...(settingsRef.current as object) } as Record<string, unknown>;
        const prevValues = prev as unknown as Record<string, unknown>;
        for (const key of Object.keys(updates)) {
          reverted[key] = prevValues[key];
        }
        const restored = reverted as unknown as AppSettings;
        settingsRef.current = restored;
        setSettings(restored);
        log.error("[SettingsContext] Failed to update settings:", err);
        throw err;
      }
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
