import { useCallback, useEffect, useRef, useState } from "react";
import {
  normalizeShortcutKeys,
  validateKeyCombinationWithRules,
  ValidationPresets,
} from "@/lib/keyboard-normalizer";
import { formatBareModifierDisplay } from "./formatBareModifierDisplay";
import {
  handleHotkeyKeyDown,
  handleHotkeyKeyUp,
  type HotkeyCaptureHandlerContext,
} from "./hotkeyCaptureHandlers";
import type { BareModifierSpec, HotkeyInputProps } from "./types";

export function useHotkeyCapture({
  onChange,
  validationRules = ValidationPresets.standard(),
  onEditingChange,
  allowBareModifier = false,
  onBareModifier,
  inline = false,
}: HotkeyInputProps) {
  const [mode, setMode] = useState<"display" | "edit">("display");
  const [keys, setKeys] = useState<Set<string>>(new Set());
  const [pendingHotkey, setPendingHotkey] = useState("");
  const [pendingBareModifier, setPendingBareModifier] = useState<BareModifierSpec | null>(null);
  const [saveStatus, setSaveStatus] = useState<"idle" | "success" | "error">("idle");
  const [validationError, setValidationError] = useState<string>("");
  const [currentKeysDisplay, setCurrentKeysDisplay] = useState<string>("");
  const onChangeRef = useRef(onChange);
  const onBareModifierRef = useRef(onBareModifier);
  const keysRef = useRef(keys);
  const validationRulesRef = useRef(validationRules);
  useEffect(() => {
    onChangeRef.current = onChange;
    onBareModifierRef.current = onBareModifier;
    keysRef.current = keys;
    validationRulesRef.current = validationRules;
  });

  const handleCancel = useCallback(() => {
    setPendingHotkey("");
    setPendingBareModifier(null);
    setKeys(new Set());
    setMode("display");
    setSaveStatus("idle");
    setValidationError("");
    setCurrentKeysDisplay("");
    onEditingChange?.(false);
  }, [onEditingChange]);
  const handleCancelRef = useRef(handleCancel);
  useEffect(() => {
    handleCancelRef.current = handleCancel;
  });
  useEffect(() => {
    if (mode !== "edit" && !inline) return;

    const ctx: HotkeyCaptureHandlerContext = {
      keysRef,
      validationRulesRef,
      handleCancelRef,
      onChangeRef,
      onBareModifierRef,
      allowBareModifier,
      inline,
      setKeys,
      setValidationError,
      setPendingBareModifier,
      setPendingHotkey,
      setCurrentKeysDisplay,
    };

    const handleKeyDown = (e: KeyboardEvent) => handleHotkeyKeyDown(e, ctx);
    const handleKeyUp = (e: KeyboardEvent) => handleHotkeyKeyUp(e, ctx);

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
    };
  }, [mode, allowBareModifier, inline]);

  const handleSave = useCallback(() => {
    // ── Bare modifier save ────────────────────────────────────────────────
    if (allowBareModifier && pendingBareModifier) {
      onBareModifier?.(pendingBareModifier);
      setSaveStatus("success");
      setMode("display");
      setSaveStatus("idle");
      setPendingBareModifier(null);
      setPendingHotkey("");
      setKeys(new Set());
      setCurrentKeysDisplay("");
      onEditingChange?.(false);
      return;
    }
    // ─────────────────────────────────────────────────────────────────────

    if (pendingHotkey && !validationError) {
      // Normalize the shortcut before saving
      const normalizedShortcut = normalizeShortcutKeys(pendingHotkey);

      // Final validation
      const validation = validateKeyCombinationWithRules(normalizedShortcut, validationRules);
      if (!validation.valid) {
        setValidationError(validation.error || "Invalid key combination");
        return;
      }

      onChange(normalizedShortcut);
      setSaveStatus("success");
      setMode("display");
      setSaveStatus("idle");
      setPendingHotkey("");
      setKeys(new Set());
      setCurrentKeysDisplay("");
      onEditingChange?.(false); // Notify parent that editing is done
    }
  }, [
    allowBareModifier,
    pendingBareModifier,
    pendingHotkey,
    validationError,
    onChange,
    onBareModifier,
    onEditingChange,
    validationRules,
  ]);

  const handleEdit = useCallback(() => {
    setPendingHotkey("");
    setPendingBareModifier(null);
    setMode("edit");
    setSaveStatus("idle");
    setValidationError("");
    setCurrentKeysDisplay("");
    setKeys(new Set());
    onEditingChange?.(true); // Notify parent that editing has started
  }, [onEditingChange]);

  // Reset save status after showing success
  useEffect(() => {
    if (saveStatus === "success") {
      const timer = setTimeout(() => setSaveStatus("idle"), 3000);
      return () => clearTimeout(timer);
    }
  }, [saveStatus]);

  // Edit mode: the bare modifier display takes priority over the combo preview.
  const editLabel: string | null = pendingBareModifier
    ? formatBareModifierDisplay(pendingBareModifier)
    : null;

  const canSave =
    (allowBareModifier && !!pendingBareModifier) || (!!pendingHotkey && !validationError);

  return {
    mode,
    pendingHotkey,
    pendingBareModifier,
    saveStatus,
    validationError,
    currentKeysDisplay,
    handleCancel,
    handleSave,
    handleEdit,
    editLabel,
    canSave,
  };
}
