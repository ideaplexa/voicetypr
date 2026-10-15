import { useTauriEvent } from "@/hooks/useTauriEvent";
import type { MainNavigate } from "@/components/app/mainNavigation";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { OverviewTab } from "@/components/tabs/OverviewTab";
import { RecordingsTab } from "@/components/tabs/RecordingsTab";
import { SettingsModal } from "@/components/SettingsModal";
import { isMacOS } from "@/lib/platform";
import { AudioUploadSection } from "@/components/sections/AudioUploadSection";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { resolveScreen, type ScreenId, type SettingsPane } from "@/components/navigation";
import type { SourceFilterProps } from "@/components/sections/models/types";

const InsightsTab = lazy(() =>
  import("@/components/tabs/InsightsTab").then((m) => ({ default: m.InsightsTab })),
);
const EnhancementsTab = lazy(() =>
  import("@/components/tabs/EnhancementsTab").then((m) => ({ default: m.EnhancementsTab })),
);
const DictionarySection = lazy(() =>
  import("@/components/sections/DictionarySection").then((m) => ({ default: m.DictionarySection })),
);
const ModelsTab = lazy(() =>
  import("@/components/tabs/ModelsTab").then((m) => ({ default: m.ModelsTab })),
);
const RecordingTab = lazy(() =>
  import("@/components/tabs/RecordingTab").then((m) => ({ default: m.RecordingTab })),
);
const ReportProblemSection = lazy(() =>
  import("@/components/sections/ReportProblemSection").then((m) => ({
    default: m.ReportProblemSection,
  })),
);

/** Same-size empty slot while a secondary screen's chunk loads (no layout jump). */
function ScreenFallback() {
  return <div className="flex-1" />;
}

interface TabContainerProps extends SourceFilterProps {
  activeSection: ScreenId;
  onNavigate?: (section: ScreenId) => void;
  settingsPane?: SettingsPane;
  onSettingsClose?: () => void;
  onSettingsPaneChange?: (pane: SettingsPane) => void;
}

export function TabContainer({
  activeSection,
  onNavigate,
  settingsPane,
  onSettingsPaneChange,
  onSettingsClose,
  ...sourceFilterProps
}: TabContainerProps) {
  const incoming = resolveScreen(activeSection);
  const [route, setRoute] = useState({
    id: activeSection,
    paneProp: settingsPane,
    page: incoming.screen === "settings" ? ("home" as ScreenId) : activeSection,
    pane:
      incoming.screen === "settings" ? (incoming.pane ?? settingsPane ?? "general") : settingsPane,
  });
  if (route.id !== activeSection || route.paneProp !== settingsPane) {
    setRoute({
      id: activeSection,
      paneProp: settingsPane,
      page: incoming.screen === "settings" ? route.page : activeSection,
      pane:
        incoming.screen === "settings"
          ? (incoming.pane ?? settingsPane ?? "general")
          : settingsPane,
    });
  }
  const destination = resolveScreen(route.page);
  const onSettingsPaneChangeRef = useRef(onSettingsPaneChange);
  useEffect(() => {
    onSettingsPaneChangeRef.current = onSettingsPaneChange;
  });
  const openPane = useCallback((pane: SettingsPane) => {
    setRoute((current) => ({ ...current, pane }));
    onSettingsPaneChangeRef.current?.(pane);
  }, []);
  const closeSettings = () => {
    setRoute((current) => ({ ...current, pane: undefined }));
    onSettingsClose?.();
  };
  const navigate = (id: ScreenId) => {
    const next = resolveScreen(id);
    if (next.screen === "settings") openPane(next.pane ?? "general");
    else {
      closeSettings();
      onNavigate?.(id);
    }
  };
  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (
        event.key !== "," ||
        event.altKey ||
        event.shiftKey ||
        event.repeat ||
        !document.hasFocus()
      )
        return;
      if (isMacOS ? !event.metaKey || event.ctrlKey : !event.ctrlKey || event.metaKey) return;
      event.preventDefault();
      openPane("general");
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [openPane]);

  let content;
  switch (destination.screen) {
    case "home":
      content = (
        <OverviewTab
          onNavigate={navigate}
          onNavigateSettingsPane={openPane}
          onSourceFilterChange={sourceFilterProps.onSourceFilterChange}
        />
      );
      break;
    case "history":
      content = (
        <HistoryContent
          key={route.page}
          initialUploadOpen={route.page === "audio"}
          onNavigate={navigate}
          onNavigateSettingsPane={openPane}
          onSourceFilterChange={sourceFilterProps.onSourceFilterChange}
        />
      );
      break;
    case "insights":
      content = <InsightsTab />;
      break;
    case "transcription":
      content = <ModelsTab {...sourceFilterProps} />;
      break;
    case "polish":
      content = <EnhancementsTab />;
      break;
    case "dictionary":
      content = <DictionarySection />;
      break;
    case "recording":
      content = <RecordingTab />;
      break;
    case "help":
      content = <ReportProblemSection onNavigateSettingsPane={openPane} />;
      break;
    default:
      content = (
        <OverviewTab
          onNavigate={navigate}
          onNavigateSettingsPane={openPane}
          onSourceFilterChange={sourceFilterProps.onSourceFilterChange}
        />
      );
  }
  return (
    <div className="flex h-full min-h-0 flex-col">
      <Suspense fallback={<ScreenFallback />}>{content}</Suspense>
      <SettingsModal
        pane={onSettingsClose ? settingsPane : route.pane}
        onPaneChange={openPane}
        onClose={closeSettings}
        onNavigate={navigate}
      />
    </div>
  );
}

function HistoryContent({
  initialUploadOpen,
  onNavigate,
  onNavigateSettingsPane,
  onSourceFilterChange,
}: {
  initialUploadOpen: boolean;
  onNavigate?: (screen: ScreenId) => void;
  onNavigateSettingsPane?: (pane: SettingsPane) => void;
  onSourceFilterChange?: SourceFilterProps["onSourceFilterChange"];
}) {
  const [uploadOpen, setUploadOpen] = useState(initialUploadOpen);
  useTauriEvent<MainNavigate>("main-navigate", ({ screen }) => {
    if (screen === "audio") setUploadOpen(true);
  });
  return (
    <>
      <RecordingsTab
        onTranscribeFile={() => setUploadOpen(true)}
        onNavigate={onNavigate}
        onNavigateSettingsPane={onNavigateSettingsPane}
        onSourceFilterChange={onSourceFilterChange}
      />
      <Dialog open={uploadOpen} onOpenChange={setUploadOpen}>
        <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden bg-card p-0 text-card-foreground sm:max-w-3xl">
          <DialogHeader className="shrink-0 px-6 pb-4 pt-6 pr-14">
            <DialogTitle>Transcribe a file</DialogTitle>
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
