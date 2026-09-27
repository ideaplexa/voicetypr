import React from "react";
import {
  HotkeyDisplayView,
  HotkeyEditView,
  HotkeyInlineView,
} from "./hotkey-input/HotkeyInputViews";
import { useHotkeyCapture } from "./hotkey-input/useHotkeyCapture";
import type { BareModifierSpec, HotkeyInputProps } from "./hotkey-input/types";

export type { BareModifierSpec };

export const HotkeyInput = React.memo(function HotkeyInput(props: HotkeyInputProps) {
  const { value, placeholder, inline = false } = props;
  const {
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
  } = useHotkeyCapture(props);

  if (inline) {
    return (
      <HotkeyInlineView
        pendingBareModifier={pendingBareModifier}
        pendingHotkey={pendingHotkey}
        currentKeysDisplay={currentKeysDisplay}
        value={value}
        placeholder={placeholder}
        validationError={validationError}
      />
    );
  }

  if (mode === "display") {
    return (
      <HotkeyDisplayView
        value={value}
        placeholder={placeholder}
        saveStatus={saveStatus}
        onEdit={handleEdit}
      />
    );
  }

  return (
    <HotkeyEditView
      editLabel={editLabel}
      pendingHotkey={pendingHotkey}
      currentKeysDisplay={currentKeysDisplay}
      canSave={canSave}
      validationError={validationError}
      onSave={handleSave}
      onCancel={handleCancel}
    />
  );
});
