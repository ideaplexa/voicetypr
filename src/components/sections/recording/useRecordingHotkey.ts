import type { BareModifierSpec } from "@/components/HotkeyInput";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { loadEffectivePrimaryShortcut, type EffectivePrimaryShortcut } from "@/lib/primary-shortcut";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

const log = createLogger("recording-settings");

export function useRecordingHotkey() {
  const { settings, refreshSettings } = useSettings();
  const [effective, setEffective] = useState<EffectivePrimaryShortcut | null>(null);
  const [isEditingHotkey, setIsEditingHotkey] = useState(false);
  const [pendingHotkey, setPendingHotkey] = useState("");
  const [pendingBareModifier, setPendingBareModifier] = useState<BareModifierSpec | null>(null);
  const [holdToTalk, setHoldToTalk] = useState(false);
  const [modeEdited, setModeEdited] = useState(false);

  const refreshPrimary = useCallback(async () => {
    const result = await loadEffectivePrimaryShortcut();
    setEffective(result);
    return result;
  }, []);
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    const refresh = () => {
      void loadEffectivePrimaryShortcut()
        .then((result) => { if (!cancelled) setEffective(result); })
        .catch(() => undefined);
    };
    refresh();
    void listen("shortcut-settings-changed", refresh).then((dispose) => {
      if (cancelled) dispose(); else unlisten = dispose;
    }).catch(() => undefined);
    return () => { cancelled = true; unlisten?.(); };
  }, [settings?.hotkey]);

  const startEditing = () => {
    if (!settings) return;
    if (!effective) void refreshPrimary().then((current) => setHoldToTalk(current.mode === "hold")).catch(() => undefined);
    setPendingHotkey(effective?.binding ? "" : settings.hotkey || "");
    setPendingBareModifier(null);
    setHoldToTalk(effective?.mode === "hold");
    setModeEdited(false);
    setIsEditingHotkey(true);
  };
  const handleCancelHotkey = () => {
    setIsEditingHotkey(false);
    setPendingHotkey("");
    setPendingBareModifier(null);
  };
  const setPrimary = async (kind: "combo" | "bare_modifier", value: string, mode: "hold" | "toggle") => {
    const result = await invoke<EffectivePrimaryShortcut>("set_primary_recording_shortcut", {
      request: { kind, value, mode },
    });
    setEffective(result);
    await refreshSettings();
  };
  const changeRecordingMode = async (mode: "toggle" | "push_to_talk") => {
    if (!settings) return;
    try {
      const current = effective ?? await refreshPrimary();
      const modifier = current.binding?.modifier;
      if (modifier) {
        await setPrimary("bare_modifier", `${modifier.modifier}:${modifier.side}`, mode === "push_to_talk" ? "hold" : "toggle");
      } else if (current.hotkey) {
        await setPrimary("combo", current.hotkey, mode === "push_to_talk" ? "hold" : "toggle");
      }
    } catch (error) {
      log.error("Failed to update recording mode:", error);
      toast.error("Failed to update recording mode.");
    }
  };
  const handleSaveHotkey = async () => {
    if (!settings || (!pendingHotkey && !pendingBareModifier)) return;
    try {
      if (pendingBareModifier) {
        const current = effective ?? await refreshPrimary();
        await setPrimary("bare_modifier", `${pendingBareModifier.modifier}:${pendingBareModifier.side}`, modeEdited ? (holdToTalk ? "hold" : "toggle") : current.mode);
      } else {
        // The backend captures the real primary and its mode before any write.
        const current = effective ?? await refreshPrimary();
        await setPrimary("combo", pendingHotkey, current.mode);
      }
      setIsEditingHotkey(false);
      setPendingHotkey("");
      setPendingBareModifier(null);
      toast.success("Hotkey updated successfully!");
    } catch (error) {
      log.error("Failed to update hotkey:", error);
      toast.error(error instanceof Error ? error.message : String(error));
    }
  };

  return {
    effective,
    nativeBinding: effective?.binding ?? null,
    isEditingHotkey, pendingHotkey, setPendingHotkey,
    pendingBareModifier, setPendingBareModifier,
    holdToTalk, setHoldToTalk: (value: boolean) => { setHoldToTalk(value); setModeEdited(true); }, startEditing,
    handleCancelHotkey, handleSaveHotkey, changeRecordingMode,
  };
}
