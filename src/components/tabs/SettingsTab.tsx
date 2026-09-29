import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import {
  SettingsPaneCard,
  SettingsPaneHeader,
  SettingsPaneRow,
} from "@/components/settings/settings-ui";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import { updateService } from "@/services/updateService";
import { GeneralSettings } from "@/components/sections/GeneralSettings";
import { ShortcutsSection } from "@/components/sections/ShortcutsSection";
import { TelemetrySection } from "@/components/sections/TelemetrySection";
import { StorageCleanupCard } from "@/components/sections/recording/StorageCleanupCard";
import { AdvancedSection } from "@/components/sections/AdvancedSection";
import { NetworkSharingCard } from "@/components/sections/NetworkSharingCard";
import { AgentCliSection } from "@/components/sections/AgentCliSection";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { isMacOS } from "@/lib/platform";
import { toast } from "sonner";
import {
  HardDrive,
  Keyboard,
  Settings2,
  Share2,
  ShieldCheck,
  Terminal,
  Wrench,
} from "lucide-react";
import { cn } from "@/lib/utils";

const log = createLogger("settings");
const panes = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "shortcuts", label: "Shortcuts", icon: Keyboard },
  { id: "privacy", label: "Privacy", icon: ShieldCheck },
  { id: "storage", label: "Storage", icon: HardDrive },
  { id: "network", label: "Network sharing", icon: Share2 },
  { id: "agent", label: "CLI & API", icon: Terminal },
  { id: "advanced", label: "Troubleshooting", icon: Wrench },
] as const;

export function SettingsTab({
  pane,
  onPaneChange,
  onNavigate,
}: {
  pane: SettingsPane;
  onPaneChange: (pane: SettingsPane) => void;
  onNavigate?: (screen: ScreenId) => void;
}) {
  return (
    <div className="h-full min-h-0 overflow-auto px-9 pb-7 pt-10">
      <div className="mx-auto w-full max-w-[880px]">
        <h1 className="mb-[18px] text-[24px] font-semibold tracking-[-0.4px]">Settings</h1>
        <div className="flex flex-col gap-7 md:flex-row">
          <nav
            aria-label="Settings panes"
            className="flex w-full shrink-0 flex-col gap-0.5 md:w-[170px]"
          >
            {panes.map((entry, index) => {
              const Icon = entry.icon;
              return (
                <div key={entry.id}>
                  {index === 4 ? (
                    <p className="px-2.5 pb-1 pt-[14px] text-[10.5px] font-semibold uppercase tracking-[0.6px] text-muted-foreground">
                      Advanced
                    </p>
                  ) : null}
                  <button
                    type="button"
                    onClick={() => onPaneChange(entry.id)}
                    aria-current={pane === entry.id ? "page" : undefined}
                    className={cn(
                      "flex w-full items-center gap-[9px] rounded-lg px-2.5 py-[7px] text-left text-sm font-medium text-muted-foreground hover:bg-muted",
                      pane === entry.id && "bg-muted font-semibold text-foreground",
                    )}
                  >
                    <Icon className="size-[15px]" />
                    {entry.label}
                  </button>
                </div>
              );
            })}
          </nav>
          <div
            className={cn(
              "flex min-w-0 flex-1 flex-col",
              pane === "privacy" || pane === "shortcuts" ? "gap-[14px]" : "gap-3",
            )}
          >
            {pane === "general" ? (
              <>
                <SettingsPaneHeader
                  title="General"
                  description="Make Voicetypr feel right on this computer."
                />
                <GeneralSettings embedded />
                <AboutCard />
              </>
            ) : null}
            {pane === "shortcuts" ? (
              <>
                <SettingsPaneHeader
                  title="Shortcuts"
                  description="Extra shortcuts for app actions. Your dictation shortcut lives in Recording."
                />
                <ShortcutsSection onNavigateRecording={() => onNavigate?.("recording")} />
              </>
            ) : null}
            {pane === "privacy" ? (
              <>
                <SettingsPaneHeader
                  title="Privacy"
                  description={`Everything you dictate stays on this ${isMacOS ? "Mac" : "PC"} unless you choose a cloud engine or AI provider.`}
                />
                <TelemetrySection />
              </>
            ) : null}
            {pane === "storage" ? (
              <>
                <SettingsPaneHeader
                  title="Storage"
                  description="Where your history and recordings live, and how long they stay."
                />
                <SettingsPaneCard title="History">
                  <StorageCleanupCard pane />
                </SettingsPaneCard>
                <StorageFilesCard onNavigate={onNavigate} />
              </>
            ) : null}
            {pane === "network" ? (
              <>
                <SettingsPaneHeader
                  title="Network sharing"
                  description="Let your other computers use this Voicetypr to transcribe. Local network only."
                />
                <NetworkSharingCard pane />
              </>
            ) : null}
            {pane === "agent" ? (
              <>
                <SettingsPaneHeader
                  title="CLI & API"
                  description="Use Voicetypr from the terminal and from your AI agents."
                />
                <AgentCliSection />
              </>
            ) : null}
            {pane === "advanced" ? <AdvancedSection embedded /> : null}
          </div>
        </div>
      </div>
    </div>
  );
}

function StorageFilesCard({ onNavigate }: { onNavigate?: (screen: ScreenId) => void }) {
  const { settings } = useSettings();
  const [folder, setFolder] = useState<string | null>(null);
  useEffect(() => {
    void invoke<string>("get_recordings_directory")
      .then(setFolder)
      .catch(() => setFolder(null));
  }, []);
  return (
    <SettingsPaneCard title="Files">
      {settings?.save_recordings ? (
        <SettingsPaneRow
          title="Recordings folder"
          description={<span className="font-mono">{folder ?? "Recordings"}</span>}
          control={
            <Button
              variant="outline"
              size="sm"
              onClick={async () => {
                try {
                  await invoke("open_recordings_folder");
                } catch (error) {
                  log.error("Failed to open recordings folder:", error);
                  toast.error("Failed to open recordings folder");
                }
              }}
            >
              Open folder
            </Button>
          }
        />
      ) : null}
      <SettingsPaneRow
        title="Models"
        description="Downloaded transcription models are managed in Transcription."
        control={
          <Button variant="outline" size="sm" onClick={() => onNavigate?.("transcription")}>
            Manage in Transcription
          </Button>
        }
      />
    </SettingsPaneCard>
  );
}

function AboutCard() {
  const [version, setVersion] = useState("—");
  const [checking, setChecking] = useState(false);
  useEffect(() => {
    void getVersion()
      .then(setVersion)
      .catch(() => setVersion("—"));
  }, []);
  const checkUpdates = async () => {
    setChecking(true);
    try {
      await updateService.checkForUpdatesManually();
    } finally {
      setChecking(false);
    }
  };
  return (
    <SettingsPaneCard title="About">
      <SettingsPaneRow
        title={`Voicetypr ${version}`}
        description="Your installed version"
        control={
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => void checkUpdates()}
            disabled={checking}
          >
            Check for updates
          </Button>
        }
      />
    </SettingsPaneCard>
  );
}
