import { Button } from "@/components/settings/SettingsButton";
import { SettingRow, SettingsPaneRow } from "@/components/settings/settings-ui";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useSettings } from "@/contexts/SettingsContext";
import { recordingAudioDescription, recordingAudioUpdate, recordingAudioValue } from "@/lib/recording-audio-setting";
import { createLogger } from "@/lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen } from "lucide-react";
import { toast } from "sonner";

const log = createLogger("recording-settings");
const periods = [
  { value: "forever", label: "Keep forever" },
  { value: "7", label: "7 days" },
  { value: "14", label: "14 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
];

export function StorageCleanupCard({ pane = false }: { pane?: boolean } = {}) {
  const Row = pane ? SettingsPaneRow : SettingRow;
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;
  const audioValue = recordingAudioValue(settings);
  return (
    <>
      <Row
        className={pane ? "min-h-[76px]" : undefined}
        title={pane ? "Keep history" : "Transcript history cleanup"}
        description={
          pane
            ? "Automatically remove old transcripts after a set number of days."
            : "Automatically remove old transcript history after a set number of days."
        }
        control={
          <Select
            items={periods}
            value={
              settings.transcription_cleanup_days == null
                ? "forever"
                : String(settings.transcription_cleanup_days)
            }
            onValueChange={(value) => {
              if (value != null)
                void updateSettings({
                  transcription_cleanup_days: value === "forever" ? null : parseInt(value, 10),
                });
            }}
          >
            <SelectTrigger aria-label="Transcript history cleanup" className="w-40">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {periods.map(({ value, label }) => (
                <SelectItem key={value} value={value}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        }
      />
      <Row
        className={pane ? "min-h-[60px]" : undefined}
        title={pane ? "Keep audio recordings" : "Save recording audio"}
        description={recordingAudioDescription(audioValue)}
        control={
          <div className="flex flex-wrap items-center gap-2">
            <Select
              items={[{ value: "off", label: "Don't save" }, { value: "failed", label: "Only failed" }, ...periods]}
              value={audioValue}
              onValueChange={async (value) => {
                if (value == null) return;
                await updateSettings(recordingAudioUpdate(value));
                if (value !== "off") toast.success(value === "failed" ? "Only failed takes will be saved" : "Recording audio will now be saved");
              }}
            >
              <SelectTrigger aria-label="Save recording audio" className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="off">Don&apos;t save</SelectItem>
                <SelectItem value="failed">Only failed</SelectItem>
                {periods.map(({ value, label }) => (
                  <SelectItem key={value} value={value}>
                    {label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {settings.save_recordings && !pane ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={async () => {
                  try {
                    await invoke("open_recordings_folder");
                  } catch {
                    log.error("Failed to open recordings folder");
                    toast.error("Failed to open recordings folder");
                  }
                }}
              >
                <FolderOpen className="size-4" />
                Open folder
              </Button>
            ) : null}
          </div>
        }
      />
    </>
  );
}
