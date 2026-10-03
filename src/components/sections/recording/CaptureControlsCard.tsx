import { HotkeyInput } from "@/components/HotkeyInput";
import { KeyCaps } from "@/components/KeyCaps";
import { MicrophoneSelection } from "@/components/MicrophoneSelection";
import { Button } from "@/components/settings/SettingsButton";
import { Switch } from "@/components/settings/SettingsSwitch";
import { Segmented, SettingsCard } from "@/components/settings/settings-ui";
import { useCanAutoInsert } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { formatPrimaryHotkeyLabel } from "@/lib/shortcut-display";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { AlertCircle, Check, X } from "lucide-react";
import { toast } from "sonner";
import { useRecordingHotkey } from "./useRecordingHotkey";

const log = createLogger("recording-settings");

export function CaptureControlsCard() {
  const { settings } = useSettings();
  const canAutoInsert = useCanAutoInsert();
  const [recording, setRecording] = useState(false);
  const [level, setLevel] = useState(0);
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void invoke<{ state: string }>("get_current_recording_state")
      .then((current) => {
        if (!disposed) setRecording(current.state === "recording");
      })
      .catch(() => undefined);
    void listen<{ state: string }>("recording-state-changed", (event) => {
      if (!disposed) {
        setRecording(event.payload.state === "recording");
        if (event.payload.state !== "recording") setLevel(0);
      }
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else unlisteners.push(unlisten);
      })
      .catch(() => undefined);
    void listen<number | { level: number }>("audio-level", (event) => {
      const raw = typeof event.payload === "number" ? event.payload : event.payload.level;
      if (!disposed) setLevel(Math.max(0, Math.min(1, raw)));
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else unlisteners.push(unlisten);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);
  const {
    effective,
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
    changeRecordingMode,
  } = useRecordingHotkey();
  if (!settings) return null;
  const hotkey = effective?.hotkey ?? (effective ? undefined : settings.hotkey);
  const label = !effective && !settings.hotkey ? "Loading shortcut…" : formatPrimaryHotkeyLabel(nativeBinding, hotkey ?? undefined);
  const caps = hotkey
    ? shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows")
    : [label];
  return (
    <div className="grid gap-[14px] sm:grid-cols-2">
      <SettingsCard title="Dictation shortcut">
        <div className="mt-3 flex flex-wrap items-center gap-2.5 rounded-[10px] bg-muted px-[14px] py-3">
          {isEditingHotkey ? (
            <div className="w-full space-y-2">
              <HotkeyInput
                inline
                value={pendingHotkey}
                onChange={(value) => {
                  setPendingHotkey(value);
                  setPendingBareModifier(null);
                }}
                allowBareModifier
                onBareModifier={(spec) => {
                  setPendingBareModifier(spec);
                  setPendingHotkey("");
                }}
                placeholder="Press a key..."
              />
              {pendingBareModifier ? (
                <label className="flex items-center gap-2 text-xs">
                  <Switch checked={holdToTalk} onCheckedChange={setHoldToTalk} id="hold-to-talk" />
                  Hold to talk (push-to-talk)
                </label>
              ) : null}
              <div className="flex gap-2">
                <Button
                  size="sm"
                  onClick={handleSaveHotkey}
                  disabled={!pendingHotkey && !pendingBareModifier}
                >
                  <Check className="size-3" />
                  Save
                </Button>
                <Button size="sm" variant="outline" onClick={handleCancelHotkey}>
                  <X className="size-3" />
                  Cancel
                </Button>
              </div>
            </div>
          ) : (
            <>
              <div
                className="flex flex-1 flex-wrap gap-1.5"
                aria-label={`Current shortcut: ${label}`}
              >
                <KeyCaps caps={caps} />
              </div>
              <Button size="sm" variant="ghost" className="text-sage" onClick={startEditing}>
                Change
              </Button>
            </>
          )}
        </div>
        <div className="mt-3">
          <Segmented
            label="Recording mode"
            value={effective?.mode === "hold" ? "push_to_talk" : "toggle"}
            onValueChange={(value) => {
              void changeRecordingMode(value as "toggle" | "push_to_talk");
            }}
            options={[
              { value: "push_to_talk", label: "Hold to talk" },
              { value: "toggle", label: "Press to start / stop" },
            ]}
          />
        </div>
        <p className="mt-2 text-xs text-muted-foreground">
          Additional app shortcuts live in Shortcuts.
        </p>
        {!canAutoInsert && isMacOS ? (
          <div className="mt-3 rounded-lg border border-warn/25 bg-warn-bg p-3 text-xs text-foreground">
            <p className="flex items-center gap-2 font-medium">
              <AlertCircle className="size-4 text-warn" />
              Accessibility permission required
            </p>
            <p className="mt-1 text-muted-foreground">
              Voicetypr needs accessibility permission for global hotkeys and auto-insert.
            </p>
            <ol className="mt-2 list-decimal pl-4 text-muted-foreground">
              <li>Open System Settings</li>
              <li>Go to Privacy &amp; Security → Accessibility</li>
              <li>Add Voicetypr and enable it</li>
            </ol>
            <Button
              variant="ghost"
              size="sm"
              className="mt-1 px-0 text-sage"
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
        ) : null}
      </SettingsCard>
      <SettingsCard title="Microphone">
        <div className="mt-3">
          <MicrophoneSelection
            value={settings.selected_microphone || undefined}
            onValueChange={async (deviceName) => {
              try {
                await invoke("set_audio_device", { deviceName: deviceName || null });
                toast.success(`Microphone changed to: ${deviceName || "Default"}`);
              } catch (error) {
                log.error("Failed to set microphone:", error);
                toast.error("Failed to change microphone");
              }
            }}
          />
        </div>
        {recording ? (
          <div
            className="mt-3 flex h-[18px] items-center gap-[3px]"
            role="meter"
            aria-label="Microphone level"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(level * 100)}
          >
            {Array.from({ length: 24 }, (_, index) => (
              <span
                key={index}
                className={`h-2 flex-1 rounded-sm ${index < Math.round(level * 24) ? "bg-sage" : "bg-muted"}`}
              />
            ))}
          </div>
        ) : null}
        <p className="mt-3 text-xs text-muted-foreground">
          Speak to test — start a dictation to see the level.
        </p>
      </SettingsCard>
    </div>
  );
}
