import type { BareModifierSpec } from "@/components/HotkeyInput";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { findActivePrimaryBinding } from "@/lib/shortcut-display";
import type {
  ModifierKind,
  ModifierSide,
  ShortcutBinding,
  ShortcutSettings,
} from "@/types/shortcuts";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { toast } from "sonner";

const log = createLogger("recording-settings");

export function useRecordingHotkey() {
  const { settings, updateSettings } = useSettings();
  const [nativeBinding, setNativeBinding] = useState<ShortcutBinding | null>(null);
  const [isEditingHotkey, setIsEditingHotkey] = useState(false);
  const [pendingHotkey, setPendingHotkey] = useState("");
  const [pendingBareModifier, setPendingBareModifier] = useState<BareModifierSpec | null>(null);
  const [holdToTalk, setHoldToTalk] = useState(false);

  // Clear the native binding whenever a custom hotkey appears — adjusted
  // during render (no sync setState in an effect).
  const [lastHotkey, setLastHotkey] = useState(settings?.hotkey);
  if (lastHotkey !== settings?.hotkey) {
    setLastHotkey(settings?.hotkey);
    if (settings?.hotkey) {
      setNativeBinding(null);
    }
  }

  useEffect(() => {
    const hotkey = settings?.hotkey;
    if (hotkey) {
      return;
    }
    let cancelled = false;
    invoke<ShortcutSettings>("get_shortcut_settings")
      .then((result) => {
        if (cancelled) return;
        setNativeBinding(findActivePrimaryBinding(result.bindings));
      })
      .catch(() => {
        if (!cancelled) setNativeBinding(null);
      });
    return () => {
      cancelled = true;
    };
  }, [settings?.hotkey]);

  const startEditing = () => {
    if (!settings) return;
    setPendingHotkey(settings.hotkey || "");
    setPendingBareModifier(null);
    setHoldToTalk(
      nativeBinding?.action === "hold_to_record" ||
        (!nativeBinding && settings.recording_mode === "push_to_talk"),
    );
    setIsEditingHotkey(true);
  };

  const handleCancelHotkey = () => {
    setIsEditingHotkey(false);
    setPendingHotkey("");
    setPendingBareModifier(null);
  };

  const handleSaveHotkey = async () => {
    if (!settings) return;
    if (pendingBareModifier) {
      try {
        const existing = await invoke<ShortcutSettings>("get_shortcut_settings");
        const existingPrimary =
          existing.bindings.find((b) => b.id === "onboarding-primary-hold") ??
          existing.bindings.find(
            (b) =>
              b.enabled &&
              (b.action === "hold_to_record" || b.action === "toggle_recording") &&
              (b.trigger_kind === "modifier_hold" || b.trigger_kind === "isolated_tap"),
          );
        const stableId = existingPrimary?.id ?? "onboarding-primary-hold";
        const newBinding: ShortcutBinding = holdToTalk
          ? {
              id: stableId,
              action: "hold_to_record",
              shortcut: "",
              trigger: "hold",
              enabled: true,
              allow_risky_combo: false,
              trigger_kind: "modifier_hold",
              modifier: {
                modifier: pendingBareModifier.modifier as ModifierKind,
                side: pendingBareModifier.side as ModifierSide,
              },
            }
          : {
              id: stableId,
              action: "toggle_recording",
              shortcut: "",
              trigger: "pressed",
              enabled: true,
              allow_risky_combo: false,
              trigger_kind: "isolated_tap",
              modifier: {
                modifier: pendingBareModifier.modifier as ModifierKind,
                side: pendingBareModifier.side as ModifierSide,
              },
            };
        const updatedBindings = existingPrimary
          ? existing.bindings.map((b) => (b.id === stableId ? newBinding : b))
          : [...existing.bindings, newBinding];
        // Clear the combo hotkey BEFORE saving the bare-modifier binding:
        // update_shortcut_settings rebuilds the engine from settings.hotkey, and
        // if the combo is still set the migration treats it as authoritative and
        // disables the bare-modifier primary we just created (onboarding clears
        // first for this exact reason).
        if (settings.hotkey) {
          await updateSettings({ hotkey: "" });
        }
        await invoke("update_shortcut_settings", {
          settings: { bindings: updatedBindings },
        });
        setNativeBinding(newBinding);
        setIsEditingHotkey(false);
        setPendingHotkey("");
        setPendingBareModifier(null);
        toast.success("Hotkey updated successfully!");
      } catch (err) {
        log.error("Failed to save bare modifier hotkey:", err);
        toast.error("Failed to save hotkey. Please try again.");
      }
    } else if (pendingHotkey) {
      try {
        await invoke("set_global_shortcut", { shortcut: pendingHotkey });
        // Replacing a bare-modifier primary with a combo: disable the existing
        // native primary binding so only the combo fires. Otherwise both the
        // native trigger and the new combo global shortcut stay active at once.
        const existing = await invoke<ShortcutSettings>("get_shortcut_settings");
        const primary = findActivePrimaryBinding(existing.bindings);
        if (primary) {
          const updatedBindings = existing.bindings.map((b) =>
            b.id === primary.id ? { ...b, enabled: false } : b,
          );
          await invoke("update_shortcut_settings", {
            settings: { bindings: updatedBindings },
          });
        }
        await updateSettings({ hotkey: pendingHotkey });
        setNativeBinding(null);
        setIsEditingHotkey(false);
        setPendingHotkey("");
        toast.success("Hotkey updated successfully!");
      } catch (err) {
        log.error("Failed to update hotkey:", err);
        const errorMessage = err instanceof Error ? err.message : String(err);
        toast.error(errorMessage || "Failed to update hotkey. Please try a different combination.");
      }
    }
  };

  return {
    nativeBinding,
    isEditingHotkey,
    pendingHotkey,
    setPendingHotkey,
    pendingBareModifier,
    setPendingBareModifier,
    holdToTalk,
    setHoldToTalk,
    startEditing,
    handleCancelHotkey,
    handleSaveHotkey,
  };
}
