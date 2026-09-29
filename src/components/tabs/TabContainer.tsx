import { useState } from "react";
import { AccountTab } from "./AccountTab";
import { EnhancementsTab } from "./EnhancementsTab";
import { ModelsTab } from "./ModelsTab";
import { OverviewTab } from "./OverviewTab";
import { RecordingsTab } from "./RecordingsTab";
import { RecordingTab } from "./RecordingTab";
import { SettingsTab } from "./SettingsTab";
import { AudioUploadSection } from "@/components/sections/AudioUploadSection";
import { ReportProblemSection } from "@/components/sections/ReportProblemSection";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { resolveScreen, type ScreenId, type SettingsPane } from "@/components/navigation";
import type { SourceFilterProps } from "@/components/sections/models/types";

interface TabContainerProps extends SourceFilterProps {
  activeSection: ScreenId;
  onNavigate?: (section: ScreenId) => void;
  settingsPane?: SettingsPane;
  onSettingsPaneChange?: (pane: SettingsPane) => void;
}

export function TabContainer({
  activeSection,
  onNavigate,
  settingsPane,
  onSettingsPaneChange,
  ...sourceFilterProps
}: TabContainerProps) {
  const destination = resolveScreen(activeSection);
  const [localSettingsPane, setLocalSettingsPane] = useState<SettingsPane>("general");

  let content;
  switch (destination.screen) {
    case "home":
      content = (
        <OverviewTab onNavigate={onNavigate} onNavigateSettingsPane={onSettingsPaneChange} onSourceFilterChange={sourceFilterProps.onSourceFilterChange} />
      );
      break;
    case "history":
      content = (
        <HistoryContent key={activeSection} initialUploadOpen={activeSection === "audio"} />
      );
      break;
    case "transcription":
      content = <ModelsTab {...sourceFilterProps} />;
      break;
    case "polish":
      content = <EnhancementsTab />;
      break;
    case "recording":
      content = <RecordingTab />;
      break;
    case "settings":
      content = (
        <SettingsTab
          pane={destination.pane ?? settingsPane ?? localSettingsPane}
          onPaneChange={(pane) => {
            setLocalSettingsPane(pane);
            onSettingsPaneChange?.(pane);
          }}
        />
      );
      break;
    case "help":
      content = <ReportProblemSection />;
      break;
    case "license":
      content = <AccountTab />;
      break;
    default:
      content = (
        <OverviewTab onNavigate={onNavigate} onNavigateSettingsPane={onSettingsPaneChange} onSourceFilterChange={sourceFilterProps.onSourceFilterChange} />
      );
  }
  return <div className="flex h-full min-h-0 flex-col">{content}</div>;
}

function HistoryContent({ initialUploadOpen }: { initialUploadOpen: boolean }) {
  const [uploadOpen, setUploadOpen] = useState(initialUploadOpen);
  return (
    <>
      <RecordingsTab onTranscribeFile={() => setUploadOpen(true)} />
      <Dialog open={uploadOpen} onOpenChange={setUploadOpen}>
        <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden bg-card p-0 text-card-foreground sm:max-w-3xl">
          <DialogHeader className="shrink-0 px-6 pb-4 pt-6 pr-14">
            <DialogTitle>Transcribe a file…</DialogTitle>
            <DialogDescription>
              Choose an audio file to transcribe with your selected source.
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-0 overflow-y-auto px-6 pb-6">
            <AudioUploadSection />
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
