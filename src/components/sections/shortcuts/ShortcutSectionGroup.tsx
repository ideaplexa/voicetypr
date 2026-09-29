import type { ShortcutAction, ShortcutActionDefinition, ShortcutBinding } from "@/types/shortcuts";
import type { Dispatch, SetStateAction } from "react";
import { ShortcutActionRow } from "./ShortcutActionRow";
import type { EditingCapture } from "./shortcutUtils";

type ShortcutSectionGroupProps = {
  pane?: boolean;
  section: string;
  sectionActions: ShortcutActionDefinition[];
  bindingsByAction: Map<ShortcutAction, ShortcutBinding[]>;
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

export function ShortcutSectionGroup({
  pane = false,
  section,
  sectionActions,
  bindingsByAction,
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
}: ShortcutSectionGroupProps) {
  const sectionBindingCount = sectionActions.reduce(
    (count, action) => count + (bindingsByAction.get(action.action)?.length ?? 0),
    0,
  );

  return (
    <section className={pane ? "px-5" : "p-4"}>
      {!pane ? (
        <div className="flex items-center justify-between gap-4">
          <h2 className="text-sm font-semibold text-foreground">{section}</h2>
          {sectionBindingCount > 0 && (
            <span className="text-xs tabular-nums text-muted-foreground">
              {sectionBindingCount} {sectionBindingCount === 1 ? "shortcut" : "shortcuts"}
            </span>
          )}
        </div>
      ) : null}

      <div className={pane ? "divide-y divide-border" : "mt-2 divide-y divide-border/60"}>
        {sectionActions.map((action) => (
          <ShortcutActionRow
            key={action.action}
            pane={pane}
            action={action}
            bindings={bindingsByAction.get(action.action) ?? []}
            editingCapture={editingCapture}
            savingBindingId={savingBindingId}
            editingDisabled={editingDisabled}
            isCapturing={isCapturing}
            addDraftBinding={addDraftBinding}
            startEditing={startEditing}
            saveEdit={saveEdit}
            cancelEdit={cancelEdit}
            deleteBinding={deleteBinding}
            setEditingCapture={setEditingCapture}
          />
        ))}
      </div>
    </section>
  );
}
