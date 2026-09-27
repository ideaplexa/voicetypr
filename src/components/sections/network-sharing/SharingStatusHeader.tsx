import { Switch } from "@/components/ui/switch";
import { AlertTriangle, Network } from "lucide-react";

interface SharingStatusHeaderProps {
  enabled: boolean;
  loading: boolean;
  activeRemoteServer: string | null;
  hasShareableModel: boolean;
  currentSelectionShareable: boolean;
  modelDisplayName: string | null;
  onToggleSharing: (checked: boolean) => void;
}

export function SharingStatusHeader({
  enabled,
  loading,
  activeRemoteServer,
  hasShareableModel,
  currentSelectionShareable,
  modelDisplayName,
  onToggleSharing,
}: SharingStatusHeaderProps) {
  return (
    <>
      <div className="px-4 py-3 border-b border-border/50">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <div className="p-1.5 rounded-md bg-sage-bg">
              <Network className="h-4 w-4 text-sage" />
            </div>
            <div>
              <h3 className="font-medium">Remote Transcription</h3>
              <p className="text-xs text-muted-foreground">
                Use this device's transcription from another Voicetypr app
              </p>
            </div>
          </div>
          <Switch
            id="network-sharing"
            checked={enabled}
            onCheckedChange={onToggleSharing}
            disabled={
              loading ||
              (!enabled &&
                (!!activeRemoteServer || !hasShareableModel || !currentSelectionShareable))
            }
          />
        </div>
      </div>

      {!hasShareableModel && !enabled && (
        <div className="px-4 py-3">
          <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/20">
            <AlertTriangle className="h-4 w-4 text-amber-500 mt-0.5 flex-shrink-0" />
            <div>
              <p className="text-sm font-medium text-amber-700 dark:text-amber-400">
                No shareable local model
              </p>
              <p className="text-xs text-amber-600 dark:text-amber-500">
                Remote sharing requires a downloaded Whisper or Parakeet model on this device.
              </p>
            </div>
          </div>
        </div>
      )}

      {hasShareableModel && !currentSelectionShareable && !enabled && !activeRemoteServer && (
        <div className="px-4 py-3">
          <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/20">
            <AlertTriangle className="h-4 w-4 text-amber-500 mt-0.5 flex-shrink-0" />
            <div>
              <p className="text-sm font-medium text-amber-700 dark:text-amber-400">
                Current model cannot be shared
              </p>
              <p className="text-xs text-amber-600 dark:text-amber-500">
                Cloud sources cannot be shared over the network. Select a downloaded Whisper or
                Parakeet model in the Models tab to enable sharing.
              </p>
            </div>
          </div>
        </div>
      )}

      {activeRemoteServer && !enabled && (
        <div className="px-4 py-3">
          <div className="flex items-start gap-2 p-3 rounded-lg bg-sage-bg/60 border border-sage/20">
            <Network className="h-4 w-4 text-sage mt-0.5 flex-shrink-0" />
            <div>
              <p className="text-sm font-medium text-foreground">Using remote Voicetypr</p>
              <p className="text-xs text-muted-foreground">
                Remote transcription is unavailable while using another Voicetypr device as your
                model source.
              </p>
            </div>
          </div>
        </div>
      )}

      {hasShareableModel &&
        currentSelectionShareable &&
        !enabled &&
        modelDisplayName &&
        !activeRemoteServer && (
          <div className="px-4 py-3">
            <p className="text-xs text-muted-foreground">
              Off — {modelDisplayName} is only available on this device.
            </p>
          </div>
        )}
    </>
  );
}
