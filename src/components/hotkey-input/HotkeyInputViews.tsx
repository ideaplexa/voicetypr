import { Check, Edit2, X } from "lucide-react";
import { Button } from "../ui/button";
import { formatHotkey } from "@/lib/hotkey-utils";
import { formatBareModifierDisplay } from "./formatBareModifierDisplay";
import type { BareModifierSpec } from "./types";

interface HotkeyInlineViewProps {
  pendingBareModifier: BareModifierSpec | null;
  pendingHotkey: string;
  currentKeysDisplay: string;
  value: string;
  placeholder?: string;
  validationError: string;
}

export function HotkeyInlineView({
  pendingBareModifier,
  pendingHotkey,
  currentKeysDisplay,
  value,
  placeholder,
  validationError,
}: HotkeyInlineViewProps) {
  return (
    <div className="min-w-0 flex-1">
      <div className="flex min-h-8 items-center font-mono text-sm">
        {pendingBareModifier ? (
          <span className="text-foreground">{formatBareModifierDisplay(pendingBareModifier)}</span>
        ) : pendingHotkey ? (
          formatHotkey(pendingHotkey)
        ) : currentKeysDisplay ? (
          <span className="text-foreground">{currentKeysDisplay}</span>
        ) : value ? (
          formatHotkey(value)
        ) : (
          <span className="text-muted-foreground">{placeholder || "Press keys…"}</span>
        )}
      </div>
      {validationError && <p className="mt-1 text-xs text-destructive">{validationError}</p>}
    </div>
  );
}

interface HotkeyDisplayViewProps {
  value: string;
  placeholder?: string;
  saveStatus: "idle" | "success" | "error";
  onEdit: () => void;
}

export function HotkeyDisplayView({
  value,
  placeholder,
  saveStatus,
  onEdit,
}: HotkeyDisplayViewProps) {
  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <div className="flex-1 flex items-center">
          {value ? (
            formatHotkey(value)
          ) : (
            <span className="text-muted-foreground">{placeholder || "No hotkey set"}</span>
          )}
        </div>
        <Button size="icon" onClick={onEdit} title="Change hotkey">
          <Edit2 />
        </Button>
      </div>
      {saveStatus === "success" && (
        <div className="flex items-center gap-1 text-sm text-green-600">
          <Check className="w-3 h-3" />
          <span>Hotkey updated successfully</span>
        </div>
      )}
    </div>
  );
}

interface HotkeyEditViewProps {
  editLabel: string | null;
  pendingHotkey: string;
  currentKeysDisplay: string;
  canSave: boolean;
  validationError: string;
  onSave: () => void;
  onCancel: () => void;
}

export function HotkeyEditView({
  editLabel,
  pendingHotkey,
  currentKeysDisplay,
  canSave,
  validationError,
  onSave,
  onCancel,
}: HotkeyEditViewProps) {
  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <div className="flex-1 flex items-center">
          {editLabel ? (
            <span className="text-foreground">{editLabel}</span>
          ) : pendingHotkey ? (
            formatHotkey(pendingHotkey)
          ) : currentKeysDisplay ? (
            <span className="text-foreground">{currentKeysDisplay}</span>
          ) : (
            <span className="text-muted-foreground">Press keys to set hotkey</span>
          )}
        </div>
        <Button
          size="icon"
          variant="default"
          onClick={onSave}
          disabled={!canSave}
          title="Save hotkey"
        >
          <Check className="w-4 h-4" />
        </Button>
        <Button size="icon" variant="outline" onClick={onCancel} title="Cancel">
          <X className="w-4 h-4" />
        </Button>
      </div>
      {canSave && !validationError && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-muted-foreground">Click ✓ to save</span>
        </div>
      )}
      {validationError && (
        <div className="flex items-center gap-2">
          <span className="text-xs text-destructive">{validationError}</span>
        </div>
      )}
    </div>
  );
}
