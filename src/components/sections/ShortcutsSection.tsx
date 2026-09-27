import { SettingsHeader, SettingsPage } from "@/components/settings/settings-ui";
import { Spinner } from "@/components/ui/spinner";
import { AlertTriangle } from "lucide-react";
import { ShortcutSectionGroup } from "./shortcuts/ShortcutSectionGroup";
import { MAX_SINGLE_KEY_BINDINGS } from "./shortcuts/shortcutUtils";
import { useShortcutsSection } from "./shortcuts/useShortcutsSection";

export function ShortcutsSection() {
  const {
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
  } = useShortcutsSection();

  return (
    <SettingsPage>
      <SettingsHeader
        title="Shortcuts"
        description="Additional shortcuts for history, Polish, the dashboard, and other app actions."
      />

      <div className="rounded-2xl border border-border bg-muted/40 p-4 text-sm text-muted-foreground">
        <div className="flex gap-2">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-sage" />
          <div>
            <p>Your primary recording shortcut is configured in Recording.</p>
            <p className="mt-1">
              Voicetypr tests global shortcuts and refuses combos already owned by macOS, Windows,
              or another app.
            </p>
          </div>
        </div>
        {singleKeyCount > 0 && (
          <p className="mt-1 text-xs">
            {singleKeyCount} of {MAX_SINGLE_KEY_BINDINGS} single-key shortcuts used.
          </p>
        )}
      </div>

      {actionLoadError && (
        <div
          role="alert"
          className="rounded-2xl border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive"
        >
          Shortcut actions could not be loaded: {actionLoadError}. You can still review saved
          shortcuts once the app reconnects.
        </div>
      )}

      {settingsLoadError && (
        <div
          role="alert"
          className="rounded-2xl border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive"
        >
          Shortcut settings could not be loaded: {settingsLoadError}. Reload shortcut settings
          before editing; controls are read-only to avoid overwriting existing shortcuts.
        </div>
      )}

      {loading ? (
        <div className="flex items-center gap-2 rounded-2xl border border-border bg-card p-4 text-sm text-muted-foreground">
          <Spinner className="h-4 w-4" />
          Loading shortcuts…
        </div>
      ) : groupedActions.length === 0 ? (
        <div className="rounded-2xl border border-border bg-card p-6 text-sm text-muted-foreground">
          No shortcut actions are available.
        </div>
      ) : (
        <div className="divide-y divide-border/70 rounded-xl border border-border bg-card">
          {groupedActions.map(([section, sectionActions]) => (
            <ShortcutSectionGroup
              key={section}
              section={section}
              sectionActions={sectionActions}
              bindingsByAction={bindingsByAction}
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
      )}
    </SettingsPage>
  );
}
