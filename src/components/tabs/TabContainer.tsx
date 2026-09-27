// Direct imports for instant desktop app experience
import { AccountTab } from "./AccountTab";
import { AdvancedTab } from "./AdvancedTab";
import { EnhancementsTab } from "./EnhancementsTab";
import { ModelsTab } from "./ModelsTab";
import { OverviewTab } from "./OverviewTab";
import { RecordingsTab } from "./RecordingsTab";
import { RecordingTab } from "./RecordingTab";
import { SettingsTab } from "./SettingsTab";
import { ShortcutsTab } from "./ShortcutsTab";
import { NetworkSharingTab } from "./NetworkSharingTab";
import { AgentCliTab } from "./AgentCliTab";
import { AudioUploadSection } from "../sections/AudioUploadSection";
import { ReportProblemSection } from "../sections/ReportProblemSection";
import type { ScreenId } from "@/components/navigation";

import type { SourceFilterProps } from "../sections/models/types";

interface TabContainerProps extends SourceFilterProps {
  activeSection: ScreenId;
  onNavigate?: (section: ScreenId) => void;
}

export function TabContainer({
  activeSection,
  onNavigate,
  ...sourceFilterProps
}: TabContainerProps) {
  const renderTabContent = () => {
    switch (activeSection) {
      case "overview":
        return <OverviewTab onNavigate={onNavigate} />;

      case "recordings":
        return <RecordingsTab />;

      case "audio":
        return <AudioUploadSection />;
      case "recording":
        return <RecordingTab />;

      case "general":
        return <SettingsTab />;

      case "shortcuts":
        return <ShortcutsTab />;

      case "models":
        return <ModelsTab {...sourceFilterProps} />;

      case "network":
        return <NetworkSharingTab />;

      case "agent":
        return <AgentCliTab />;

      case "advanced":
        return <AdvancedTab />;

      case "formatting":
        return <EnhancementsTab />;

      case "license":
        return <AccountTab />;

      case "report-problem":
        return <ReportProblemSection />;

      default:
        return <OverviewTab onNavigate={onNavigate} />;
    }
  };

  return <div className="h-full min-h-0 flex flex-col">{renderTabContent()}</div>;
}
