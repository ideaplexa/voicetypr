import { HotkeyInput } from "@/components/HotkeyInput";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { ValidationPresets } from "@/lib/keyboard-normalizer";
import type { ShortcutActionDefinition, ShortcutBinding } from "@/types/shortcuts";
import { Check, Pencil, Trash2, X } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";
import { formatBindingDisplay, singleKeyValidation, type EditingCapture } from "./shortcutUtils";

type ShortcutBindingRowProps = {
  binding: ShortcutBinding;
  action: ShortcutActionDefinition;
  editingCapture: EditingCapture | null;
  isSaving: boolean;
  editingDisabled: boolean;
  isCapturing: boolean;
  showRecordingCheckbox?: boolean;
  startEditing: (binding: ShortcutBinding) => void;
  saveEdit: () => void;
  cancelEdit: () => void;
  deleteBinding: (bindingId: string) => void;
  setEditingCapture: Dispatch<SetStateAction<EditingCapture | null>>;
};

export function ShortcutBindingRow({
  binding,
  action,
  editingCapture,
  isSaving,
  editingDisabled,
  isCapturing,
  showRecordingCheckbox = action.action === "toggle_recording" ||
    action.action === "hold_to_record",
  startEditing,
  saveEdit,
  cancelEdit,
  deleteBinding,
  setEditingCapture,
}: ShortcutBindingRowProps) {
  const isEditing = editingCapture?.bindingId === binding.id;

  if (isEditing && editingCapture) {
    return (
      <div
        key={binding.id}
        className="space-y-3 rounded-lg border border-sage/40 bg-sage-bg/40 p-3"
      >
        <div className="flex items-start gap-2">
          <HotkeyInput
            inline
            value={editingCapture.combo}
            onChange={(combo) =>
              setEditingCapture((prev) =>
                prev && prev.combo !== combo
                  ? {
                      ...prev,
                      combo,
                      bareModifier: null,
                    }
                  : prev,
              )
            }
            placeholder="Press a key or key combo"
            validationRules={
              action.allows_single_key ? singleKeyValidation : ValidationPresets.standard()
            }
            allowBareModifier={showRecordingCheckbox}
            onBareModifier={(spec) =>
              setEditingCapture((prev) =>
                prev
                  ? {
                      ...prev,
                      bareModifier: spec,
                      combo: "",
                    }
                  : prev,
              )
            }
          />
          <Button
            type="button"
            size="icon-sm"
            aria-label="Save"
            className="bg-green-600 text-white hover:bg-green-600/90"
            disabled={isSaving || (!editingCapture.combo && !editingCapture.bareModifier)}
            onClick={saveEdit}
          >
            {isSaving ? <Spinner className="h-4 w-4" /> : <Check className="h-4 w-4" />}
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Cancel"
            disabled={isSaving}
            onClick={cancelEdit}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>

        {!editingCapture.bareModifier && (
          <p className="text-xs text-muted-foreground">
            Use a key combo, a function key, or a numpad/navigation key. A bare letter or number
            won&apos;t work — it would block typing.
          </p>
        )}
      </div>
    );
  }

  return (
    <div
      key={binding.id}
      className="flex items-center justify-between gap-3 rounded-lg border border-border/70 bg-muted/20 px-2.5 py-2"
    >
      <span aria-label={`${action.label} shortcut`} className="font-mono text-sm">
        {formatBindingDisplay(binding)}
      </span>
      <div className="flex items-center gap-1">
        {isSaving && <Spinner className="mr-1 h-4 w-4 text-muted-foreground" />}
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label="Edit"
          disabled={editingDisabled || isCapturing}
          onClick={() => startEditing(binding)}
        >
          <Pencil className="h-3.5 w-3.5" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label="Remove"
          className="text-muted-foreground hover:text-destructive"
          disabled={editingDisabled || isCapturing}
          onClick={() => void deleteBinding(binding.id)}
        >
          <Trash2 className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
