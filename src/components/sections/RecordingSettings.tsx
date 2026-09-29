import { PageHeader, SettingsCard, SettingsPage } from "@/components/settings/settings-ui";
import { useSettings } from "@/contexts/SettingsContext";
import { AudioFeedbackCard, AudioFeedbackDetailRows } from "./recording/AudioFeedbackCard";
import { CaptureControlsCard } from "./recording/CaptureControlsCard";
import { RecordingGuideDialog } from "./recording/RecordingGuideDialog";
import { RecordingIndicatorCard } from "./recording/RecordingIndicatorCard";
import { TranscriptHandlingCard, PauseMediaRow } from "./recording/TranscriptHandlingCard";

export function RecordingSettings() {
  const { settings } = useSettings();
  if (!settings) return null;
  return (
    <SettingsPage className="gap-4">
      <PageHeader
        title="Recording"
        description="Your shortcut, microphone and what happens after you speak."
        action={<RecordingGuideDialog />}
      />
      <CaptureControlsCard />
      <RecordingIndicatorCard />
      <div className="grid gap-[14px] sm:grid-cols-3">
        <TranscriptHandlingCard />
        <AudioFeedbackCard />
      </div>
      <SettingsCard title="More">
        <PauseMediaRow />
        <AudioFeedbackDetailRows />
      </SettingsCard>
    </SettingsPage>
  );
}
