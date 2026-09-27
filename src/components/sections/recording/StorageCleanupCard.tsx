import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen } from "lucide-react";
import { toast } from "sonner";

const log = createLogger("recording-settings");

export function StorageCleanupCard() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;

  return (
    <FieldSet className="gap-4 rounded-xl border border-border bg-card p-4">
      <FieldLegend className="mb-1 text-base font-semibold">Storage & cleanup</FieldLegend>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldTitle>Transcript history cleanup</FieldTitle>
          <FieldDescription>
            Automatically remove old transcript history after a set number of days.
          </FieldDescription>
        </FieldContent>
        <Select
          items={[
            { value: "forever", label: "Keep forever" },
            { value: "7", label: "7 days" },
            { value: "14", label: "14 days" },
            { value: "30", label: "30 days" },
            { value: "90", label: "90 days" },
          ]}
          value={
            settings.transcription_cleanup_days == null
              ? "forever"
              : String(settings.transcription_cleanup_days)
          }
          onValueChange={async (value) => {
            if (value == null) return;
            await updateSettings({
              transcription_cleanup_days: value === "forever" ? null : parseInt(value, 10),
            });
          }}
        >
          <SelectTrigger className="w-full md:w-[190px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="forever">Keep forever</SelectItem>
            <SelectItem value="7">7 days</SelectItem>
            <SelectItem value="14">14 days</SelectItem>
            <SelectItem value="30">30 days</SelectItem>
            <SelectItem value="90">90 days</SelectItem>
          </SelectContent>
        </Select>
      </Field>

      <Field
        orientation="responsive"
        className="items-start gap-3 md:[&_[data-slot=field-content]]:pt-1"
      >
        <FieldContent>
          <FieldTitle>Save recording audio</FieldTitle>
          <FieldDescription>
            Keeps the original audio for re-transcription — including retrying a failed
            transcription from History — then automatically deletes it after your chosen period.
            With this off, failed recordings can't be retried.
          </FieldDescription>
        </FieldContent>
        <div className="w-full md:w-auto">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-end">
            <Select
              items={[
                { value: "off", label: "Don't save" },
                { value: "forever", label: "Keep forever" },
                { value: "7", label: "7 days" },
                { value: "14", label: "14 days" },
                { value: "30", label: "30 days" },
                { value: "90", label: "90 days" },
              ]}
              value={
                !settings.save_recordings
                  ? "off"
                  : settings.recording_retention_days === null
                    ? "forever"
                    : String(settings.recording_retention_days ?? 30)
              }
              onValueChange={async (value) => {
                if (value == null) return;
                if (value === "off") {
                  await updateSettings({
                    save_recordings: false,
                  });
                  return;
                }

                const days = value === "forever" ? null : parseInt(value, 10);
                await updateSettings({
                  save_recordings: true,
                  recording_retention_days: days,
                });
                toast.success("Recording audio will now be saved");
              }}
            >
              <SelectTrigger className="w-full sm:w-[190px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="off">Don&apos;t save</SelectItem>
                <SelectItem value="7">Keep for 7 days</SelectItem>
                <SelectItem value="30">Keep for 30 days</SelectItem>
                <SelectItem value="90">Keep for 90 days</SelectItem>
                <SelectItem value="forever">Keep forever</SelectItem>
              </SelectContent>
            </Select>

            {settings.save_recordings && (
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="justify-center"
                onClick={async () => {
                  try {
                    await invoke("open_recordings_folder");
                  } catch (error) {
                    log.error("Failed to open recordings folder:", error);
                    toast.error("Failed to open recordings folder");
                  }
                }}
              >
                <FolderOpen className="h-4 w-4" />
                Open folder
              </Button>
            )}
          </div>
        </div>
      </Field>
    </FieldSet>
  );
}
