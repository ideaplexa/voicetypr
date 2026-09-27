import { FieldGroup } from "@/components/ui/field";
import { useSettings } from "@/contexts/SettingsContext";
import { AudioFeedbackCard } from "./recording/AudioFeedbackCard";
import { CaptureControlsCard } from "./recording/CaptureControlsCard";
import { RecordingGuideDialog } from "./recording/RecordingGuideDialog";
import { RecordingIndicatorCard } from "./recording/RecordingIndicatorCard";
import { StorageCleanupCard } from "./recording/StorageCleanupCard";
import { TranscriptHandlingCard } from "./recording/TranscriptHandlingCard";
import { TranscriptionPerformanceCard } from "./recording/TranscriptionPerformanceCard";

export function RecordingSettings() {
  const { settings } = useSettings();

  if (!settings) return null;

  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-3.5 pb-4 pl-2 pr-4">
        <div className="mb-1 flex flex-wrap items-start gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">Recording</h1>
              <RecordingGuideDialog />
            </div>
            <p className="mt-0.5 text-sm text-muted-foreground">
              Choose how Voicetypr captures, processes, and delivers each recording.
            </p>
          </div>
        </div>

        <FieldGroup className="gap-3.5">
          <CaptureControlsCard />
          <TranscriptHandlingCard />
          <AudioFeedbackCard />
          <TranscriptionPerformanceCard />
          <RecordingIndicatorCard />
          <StorageCleanupCard />
        </FieldGroup>
      </div>
    </div>
  );
}
