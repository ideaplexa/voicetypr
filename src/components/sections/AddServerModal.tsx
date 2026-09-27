import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { FieldGroup } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
import { Server } from "lucide-react";
import {
  AddServerAuthField,
  AddServerConnectionFields,
  AddServerNameField,
  AddServerTestPanel,
} from "./AddServerModalFields";
import {
  useAddServerModal,
  type InitialServerValues,
  type SavedConnection,
} from "./useAddServerModal";

interface AddServerModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onServerAdded?: (server: SavedConnection) => void;
  editServer?: SavedConnection | null;
  initialServer?: InitialServerValues | null;
}

export function AddServerModal({
  open,
  onOpenChange,
  onServerAdded,
  editServer,
  initialServer,
}: AddServerModalProps) {
  const modal = useAddServerModal({
    open,
    onOpenChange,
    onServerAdded,
    editServer,
    initialServer,
  });

  return (
    <Dialog open={open} onOpenChange={modal.handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Server className="h-5 w-5" />
            {modal.isEditMode ? "Edit Remote Voicetypr" : "Add Remote Voicetypr"}
          </DialogTitle>
          <DialogDescription>
            {modal.isEditMode
              ? "Update connection details for this remote Voicetypr"
              : "Connect to another Voicetypr over the network"}
          </DialogDescription>
        </DialogHeader>

        <FieldGroup className="py-4">
          <AddServerConnectionFields
            host={modal.host}
            port={modal.port}
            saving={modal.saving}
            onHostChange={modal.updateHost}
            onPortChange={modal.updatePort}
          />
          <AddServerAuthField
            password={modal.password}
            saving={modal.saving}
            showPassword={modal.showPassword}
            initialServerRequiresPassword={modal.initialServerRequiresPassword}
            isEditMode={modal.isEditMode}
            hasSavedPassword={!!editServer?.has_password}
            clearSavedPassword={modal.clearSavedPassword}
            onPasswordChange={modal.updatePassword}
            onToggleShowPassword={() => modal.setShowPassword(!modal.showPassword)}
            onToggleClearSavedPassword={() => modal.setClearSavedPassword((value) => !value)}
          />
          <AddServerNameField
            name={modal.name}
            saving={modal.saving}
            onNameChange={modal.setName}
          />
          <AddServerTestPanel
            host={modal.host}
            saving={modal.saving}
            testStatus={modal.testStatus}
            testResult={modal.testResult}
            testError={modal.testError}
            isSelfConnection={modal.isSelfConnection}
            testRequiresReplacementPassword={modal.testRequiresReplacementPassword}
            onTestConnection={() => {
              void modal.handleTestConnection();
            }}
          />
        </FieldGroup>

        <DialogFooter>
          <Button variant="outline" onClick={modal.handleClose} disabled={modal.saving}>
            Cancel
          </Button>
          <Button
            onClick={() => {
              void modal.handleSaveServer();
            }}
            disabled={
              !modal.host.trim() ||
              modal.saving ||
              modal.isSelfConnection ||
              modal.initialPasswordRequirementUnmet
            }
          >
            {modal.saving ? (
              <>
                <Spinner className="size-4" />
                {modal.isEditMode ? "Saving..." : "Adding..."}
              </>
            ) : modal.isEditMode ? (
              "Save Changes"
            ) : (
              "Add Server"
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export type { InitialServerValues, SavedConnection };
