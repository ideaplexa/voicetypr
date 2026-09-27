import { HotkeyInput } from "@/components/HotkeyInput";
import { MicrophoneSelection } from "@/components/MicrophoneSelection";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import { Switch } from "@/components/ui/switch";
import { useCanAutoInsert } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { isMacOS } from "@/lib/platform";
import { formatPrimaryHotkeyLabel } from "@/lib/shortcut-display";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, Check, Edit2, X } from "lucide-react";
import { toast } from "sonner";
import { useRecordingHotkey } from "./useRecordingHotkey";

const log = createLogger("recording-settings");

export function CaptureControlsCard() {
  const { settings } = useSettings();
  const showAccessibilityWarning = isMacOS;
  const canAutoInsert = useCanAutoInsert();
  const {
    nativeBinding,
    isEditingHotkey,
    pendingHotkey,
    setPendingHotkey,
    pendingBareModifier,
    setPendingBareModifier,
    holdToTalk,
    setHoldToTalk,
    startEditing,
    handleCancelHotkey,
    handleSaveHotkey,
  } = useRecordingHotkey();

  if (!settings) return null;

  return (
    <FieldSet className="gap-4 rounded-xl border border-border bg-card p-4">
      <FieldLegend className="mb-1 text-base font-semibold">Capture controls</FieldLegend>

      <Field orientation="responsive" className="items-start gap-3">
        <FieldContent>
          <FieldTitle>Recording Hotkey</FieldTitle>
          <FieldDescription>
            {isEditingHotkey
              ? "Press a key or modifier, then save."
              : "The shortcut that starts and stops recording."}
          </FieldDescription>
        </FieldContent>
        <div className="w-full md:w-auto">
          {isEditingHotkey ? (
            <div className="space-y-3">
              <HotkeyInput
                inline
                value={pendingHotkey}
                onChange={(v) => {
                  setPendingHotkey(v);
                  setPendingBareModifier(null);
                }}
                allowBareModifier
                onBareModifier={(spec) => {
                  setPendingBareModifier(spec);
                  setPendingHotkey("");
                }}
                placeholder="Press a key..."
              />
              {pendingBareModifier && (
                <label className="flex cursor-pointer items-center gap-2 text-sm select-none">
                  <Switch checked={holdToTalk} onCheckedChange={setHoldToTalk} id="hold-to-talk" />
                  <span>Hold to talk (push-to-talk)</span>
                </label>
              )}
              <div className="flex gap-2">
                <Button
                  type="button"
                  size="sm"
                  onClick={handleSaveHotkey}
                  disabled={!pendingHotkey && !pendingBareModifier}
                >
                  <Check className="h-3.5 w-3.5" />
                  Save
                </Button>
                <Button type="button" size="sm" variant="outline" onClick={handleCancelHotkey}>
                  <X className="h-3.5 w-3.5" />
                  Cancel
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex items-center gap-2">
              <div className="flex min-h-9 items-center rounded-md border border-input bg-muted/30 px-3 text-sm">
                {formatPrimaryHotkeyLabel(nativeBinding, settings.hotkey)}
              </div>
              <Button type="button" size="sm" variant="outline" onClick={startEditing}>
                <Edit2 className="h-3.5 w-3.5" />
                Edit
              </Button>
            </div>
          )}
        </div>
      </Field>

      <p className="text-xs text-muted-foreground">
        Primary recording shortcut. Additional app shortcuts live in{" "}
        <span className="font-medium text-foreground">Shortcuts</span>.
      </p>

      {!canAutoInsert && showAccessibilityWarning && (
        <div className="rounded-lg border border-amber-500/25 bg-amber-500/10 p-3">
          <div className="flex items-start gap-2">
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600 dark:text-amber-500" />
            <div className="space-y-2">
              <div>
                <p className="text-sm font-medium text-amber-900 dark:text-amber-400">
                  Accessibility permission required
                </p>
                <p className="text-xs text-amber-800 dark:text-amber-500">
                  Voicetypr needs accessibility permission for global hotkeys and auto-insert.
                </p>
              </div>
              <ol className="list-decimal space-y-0.5 pl-4 text-xs text-amber-800 dark:text-amber-500">
                <li>Open System Settings</li>
                <li>Go to Privacy &amp; Security → Accessibility</li>
                <li>Add Voicetypr and enable it</li>
              </ol>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-auto px-0 text-amber-700 hover:bg-transparent hover:text-amber-800 dark:text-amber-400 dark:hover:text-amber-300"
                onClick={async () => {
                  try {
                    await invoke("open_accessibility_settings");
                  } catch (error) {
                    log.error("Failed to open accessibility settings:", error);
                    toast.error("Could not open settings. Please open System Settings manually.");
                  }
                }}
              >
                Open Accessibility Settings
              </Button>
            </div>
          </div>
        </div>
      )}

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldTitle>Microphone</FieldTitle>
          <FieldDescription>Select your preferred audio input device.</FieldDescription>
        </FieldContent>
        <div className="w-full md:w-auto">
          <MicrophoneSelection
            value={settings.selected_microphone || undefined}
            onValueChange={async (deviceName) => {
              try {
                await invoke("set_audio_device", {
                  deviceName: deviceName || null,
                });
                toast.success(`Microphone changed to: ${deviceName || "Default"}`);
              } catch (error) {
                log.error("Failed to set microphone:", error);
                toast.error("Failed to change microphone");
              }
            }}
          />
        </div>
      </Field>
    </FieldSet>
  );
}
