import { Button } from "@/components/ui/button";
import type { ShortcutActionDefinition, ShortcutBinding } from "@/types/shortcuts";
import { Plus } from "lucide-react";
import type { Dispatch, SetStateAction } from "react";
import { ShortcutBindingRow } from "./ShortcutBindingRow";
import type { EditingCapture } from "./shortcutUtils";

type ShortcutActionRowProps = {
  pane?: boolean;
  action: ShortcutActionDefinition;
  bindings: ShortcutBinding[];
  editingCapture: EditingCapture | null;
  savingBindingId: string | null;
  editingDisabled: boolean;
  isCapturing: boolean;
  addDraftBinding: (action: ShortcutActionDefinition) => void;
  startEditing: (binding: ShortcutBinding) => void;
  saveEdit: () => void;
  cancelEdit: () => void;
  deleteBinding: (bindingId: string) => void;
  setEditingCapture: Dispatch<SetStateAction<EditingCapture | null>>;
};

export function ShortcutActionRow({
  pane = false,
  action,
  bindings,
  editingCapture,
  savingBindingId,
  editingDisabled,
  isCapturing,
  addDraftBinding,
  startEditing,
  saveEdit,
  cancelEdit,
  deleteBinding,
  setEditingCapture,
}: ShortcutActionRowProps) {
  const isCancelRecording = action.action === "cancel_recording";

  return (
    <div
      role="group"
      aria-label={action.label}
      className={
        pane
          ? "flex min-h-[62px] items-center justify-between gap-5 py-[13px]"
          : "py-3 first:pt-1 last:pb-0"
      }
    >
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between sm:gap-6">
        <div className="min-w-0">
          <h3 className="text-sm font-medium text-foreground">{action.label}</h3>
          <p className="mt-0.5 text-[12px] leading-relaxed text-muted-foreground">
            {isCancelRecording
              ? "Press Escape twice while recording to cancel the current take."
              : action.description}
          </p>
        </div>
        {bindings.length === 0 && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="w-full shrink-0 sm:w-auto"
            disabled={editingDisabled || isCapturing}
            onClick={() => addDraftBinding(action)}
          >
            <Plus className="h-3.5 w-3.5" />
            Set shortcut
          </Button>
        )}
      </div>

      {bindings.length > 0 && (
        <div className={pane ? "flex shrink-0 flex-col gap-1" : "mt-2 flex flex-col gap-2"}>
          {bindings.map((binding) => (
            <ShortcutBindingRow
              key={binding.id}
              pane={pane}
              binding={binding}
              action={action}
              editingCapture={editingCapture}
              isSaving={savingBindingId === binding.id}
              editingDisabled={editingDisabled}
              isCapturing={isCapturing}
              startEditing={startEditing}
              saveEdit={saveEdit}
              cancelEdit={cancelEdit}
              deleteBinding={deleteBinding}
              setEditingCapture={setEditingCapture}
            />
          ))}
        </div>
      )}
    </div>
  );
}
