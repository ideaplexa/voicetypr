import { createLogger } from "@/lib/logger";
import type {
  ShortcutActionDefinition,
  ShortcutBinding,
  ShortcutSettings,
} from "@/types/shortcuts";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import {
  bindingFromEditingCapture,
  countEnabledSingleKeyBindings,
  createBinding,
  emptySettings,
  findConflictingBinding,
  formatError,
  groupActionsBySection,
  groupBindingsByAction,
  normalizeSettings,
  type EditingCapture,
} from "./shortcutUtils";

const log = createLogger("shortcuts");

export function useShortcutsSection() {
  const [actions, setActions] = useState<ShortcutActionDefinition[]>([]);
  const [settings, setSettings] = useState<ShortcutSettings>(emptySettings);
  const [draftBindings, setDraftBindings] = useState<ShortcutBinding[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoadError, setActionLoadError] = useState<string | null>(null);
  const [settingsLoadError, setSettingsLoadError] = useState<string | null>(null);
  const [savingBindingId, setSavingBindingId] = useState<string | null>(null);
  const savingBindingIdRef = useRef<string | null>(null);
  const [editingCapture, setEditingCapture] = useState<EditingCapture | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadShortcuts() {
      setLoading(true);
      try {
        const [actionResult, settingsResult] = await Promise.allSettled([
          invoke<ShortcutActionDefinition[]>("list_shortcut_actions"),
          invoke<ShortcutSettings>("get_shortcut_settings"),
        ]);

        if (cancelled) return;

        let nextActionLoadError: string | null = null;
        let nextSettingsLoadError: string | null = null;

        if (actionResult.status === "fulfilled") {
          setActions(actionResult.value);
        } else {
          log.error("Failed to load shortcut actions:", actionResult.reason);
          setActions([]);
          nextActionLoadError = formatError(actionResult.reason);
        }

        if (settingsResult.status === "fulfilled") {
          setSettings(normalizeSettings(settingsResult.value));
          nextSettingsLoadError = null;
        } else {
          log.error("Failed to load shortcut settings:", settingsResult.reason);
          setSettings(emptySettings);
          nextSettingsLoadError = formatError(settingsResult.reason);
        }

        setActionLoadError(nextActionLoadError);
        setSettingsLoadError(nextSettingsLoadError);
      } finally {
        setLoading(false);
      }
    }

    loadShortcuts();

    return () => {
      cancelled = true;
    };
  }, []);

  const groupedActions = useMemo(() => groupActionsBySection(actions), [actions]);
  const bindingsByAction = useMemo(
    () => groupBindingsByAction(settings.bindings, draftBindings),
    [draftBindings, settings.bindings],
  );
  const actionLabels = useMemo(
    () => new Map(actions.map((action) => [action.action, action.label])),
    [actions],
  );
  const singleKeyCount = useMemo(
    () => countEnabledSingleKeyBindings(settings.bindings, draftBindings),
    [settings.bindings, draftBindings],
  );

  const isMutating = savingBindingId !== null;
  const editingDisabled = isMutating || settingsLoadError !== null;
  const isCapturing = editingCapture !== null;

  const beginMutation = useCallback(
    (bindingId: string) => {
      if (savingBindingIdRef.current !== null || settingsLoadError !== null) {
        return false;
      }

      savingBindingIdRef.current = bindingId;
      setSavingBindingId(bindingId);
      return true;
    },
    [settingsLoadError],
  );

  const endMutation = useCallback(() => {
    savingBindingIdRef.current = null;
    setSavingBindingId(null);
  }, []);

  const persistSettings = useCallback(
    async (nextSettings: ShortcutSettings, successMessage: string) => {
      const savedSettings = await invoke<ShortcutSettings>("update_shortcut_settings", {
        settings: nextSettings,
      });
      setSettings(normalizeSettings(savedSettings));
      toast.success(successMessage);
    },
    [],
  );

  const updateBinding = useCallback(
    async (nextBinding: ShortcutBinding) => {
      const duplicateBinding = findConflictingBinding(
        nextBinding,
        settings.bindings,
        draftBindings,
      );
      if (duplicateBinding) {
        toast.error("Shortcut already assigned", {
          description: `${nextBinding.shortcut.trim()} is already assigned to ${actionLabels.get(duplicateBinding.action) ?? duplicateBinding.action}.`,
        });
        return;
      }

      if (!beginMutation(nextBinding.id)) {
        return;
      }

      const isDraft = draftBindings.some((binding) => binding.id === nextBinding.id);
      const nextSettings = {
        bindings: isDraft
          ? [...settings.bindings, nextBinding]
          : settings.bindings.map((binding) =>
              binding.id === nextBinding.id ? nextBinding : binding,
            ),
      };

      try {
        await persistSettings(nextSettings, "Shortcut saved.");
        if (isDraft) {
          setDraftBindings((bindings) =>
            bindings.filter((binding) => binding.id !== nextBinding.id),
          );
        }
      } catch (error) {
        log.error("Failed to save shortcut:", error);
        toast.error("Could not save shortcut", {
          description: formatError(error),
        });
      } finally {
        endMutation();
      }
    },
    [actionLabels, beginMutation, draftBindings, endMutation, persistSettings, settings.bindings],
  );

  const deleteBinding = useCallback(
    async (bindingId: string) => {
      if (editingDisabled || savingBindingIdRef.current !== null) {
        return;
      }

      if (draftBindings.some((binding) => binding.id === bindingId)) {
        setDraftBindings((bindings) => bindings.filter((binding) => binding.id !== bindingId));
        return;
      }

      if (!beginMutation(bindingId)) {
        return;
      }

      const nextSettings = {
        bindings: settings.bindings.filter((binding) => binding.id !== bindingId),
      };

      try {
        await persistSettings(nextSettings, "Shortcut removed.");
      } catch (error) {
        log.error("Failed to remove shortcut:", error);
        toast.error("Could not remove shortcut", { description: formatError(error) });
      } finally {
        endMutation();
      }
    },
    [
      beginMutation,
      draftBindings,
      editingDisabled,
      endMutation,
      persistSettings,
      settings.bindings,
    ],
  );

  const startEditing = useCallback(
    (binding: ShortcutBinding) => {
      if (editingDisabled || savingBindingIdRef.current !== null) return;
      setEditingCapture({
        bindingId: binding.id,
        combo: binding.shortcut || "",
        bareModifier: null,
        allowRiskyCombo: binding.allow_risky_combo,
      });
    },
    [editingDisabled],
  );

  const cancelEdit = useCallback(() => {
    if (!editingCapture) return;
    const isDraft = draftBindings.some((b) => b.id === editingCapture.bindingId);
    if (isDraft) {
      setDraftBindings((bindings) => bindings.filter((b) => b.id !== editingCapture.bindingId));
    }
    setEditingCapture(null);
  }, [draftBindings, editingCapture]);

  const saveEdit = useCallback(async () => {
    if (!editingCapture) return;

    const originalBinding =
      settings.bindings.find((b) => b.id === editingCapture.bindingId) ??
      draftBindings.find((b) => b.id === editingCapture.bindingId);

    if (!originalBinding) return;

    const recommendedTrigger =
      actions.find((a) => a.action === originalBinding.action)?.recommended_trigger ?? "pressed";

    setEditingCapture(null);
    await updateBinding(
      bindingFromEditingCapture(originalBinding, editingCapture, recommendedTrigger),
    );
  }, [actions, draftBindings, editingCapture, settings.bindings, updateBinding]);

  const addDraftBinding = useCallback(
    (action: ShortcutActionDefinition) => {
      if (editingDisabled || savingBindingIdRef.current !== null || isCapturing) {
        return;
      }

      const newBinding = createBinding(action);
      setDraftBindings((bindings) => [...bindings, newBinding]);
      setEditingCapture({
        bindingId: newBinding.id,
        combo: "",
        bareModifier: null,
        allowRiskyCombo: false,
      });
    },
    [editingDisabled, isCapturing],
  );

  return {
    groupedActions,
    bindingsByAction,
    singleKeyCount,
    loading,
    actionLoadError,
    settingsLoadError,
    savingBindingId,
    editingCapture,
    editingDisabled,
    isCapturing,
    addDraftBinding,
    startEditing,
    saveEdit,
    cancelEdit,
    deleteBinding,
    setEditingCapture,
  };
}
