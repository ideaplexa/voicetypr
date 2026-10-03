import { recordEvent, takeSettingsSource } from "@/lib/observability";
import { useEffect, useRef } from "react";
import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";
import { Dialog, DialogOverlay, DialogPortal } from "@/components/ui/dialog";
import { SettingsTab } from "@/components/tabs/SettingsTab";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import "@/components/settings/settings-modal.css";

export function SettingsModal({
  pane,
  onPaneChange,
  onClose,
  onNavigate,
}: {
  pane?: SettingsPane;
  onPaneChange: (pane: SettingsPane) => void;
  onClose: () => void;
  onNavigate: (screen: ScreenId) => void;
}) {
  useEffect(() => { if (pane) recordEvent({ name: "settings_opened", properties: { pane, source: takeSettingsSource() } }); }, [pane]);
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog
      open={Boolean(pane)}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogPortal>
        <DialogOverlay data-pencil-name="Scrim" className="settings-modal-scrim" />
        <DialogPrimitive.Popup
          data-slot="dialog-content"
          data-pencil-name="Settings modal"
          aria-labelledby="settings-pane-title"
          aria-describedby="settings-pane-description"
          initialFocus={() => {
            returnFocus.current =
              document.activeElement instanceof HTMLElement ? document.activeElement : null;
            return true;
          }}
          finalFocus={returnFocus}
          className="settings-modal bg-card text-foreground"
        >
          <SettingsTab
            modal
            pane={pane ?? "general"}
            onPaneChange={onPaneChange}
            onNavigate={onNavigate}
          />
        </DialogPrimitive.Popup>
      </DialogPortal>
    </Dialog>
  );
}
