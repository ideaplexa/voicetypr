import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/settings/SettingsButton";
import { SettingsPaneCard, SettingsPaneRow } from "@/components/settings/settings-ui";
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
import { toast } from "sonner";
import { isMacOS } from "@/lib/platform";
import {
  HardDrive,
  Keyboard,
  Settings2,
  Share2,
  ShieldCheck,
  Terminal,
  Wrench,
  BadgeCheck,
  Info,
  X,
} from "lucide-react";
import { AccountTab } from "@/components/tabs/AccountTab";
import { UpdateAnnouncementDialog } from "@/components/UpdateAnnouncementDialog";
import { DialogClose } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

const log = createLogger("settings");
export const settingsPanes = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "shortcuts", label: "Shortcuts", icon: Keyboard },
  { id: "privacy", label: "Privacy", icon: ShieldCheck },
  { id: "storage", label: "Storage", icon: HardDrive },
  { id: "network", label: "Network sharing", icon: Share2 },
  { id: "agent", label: "CLI & API", icon: Terminal },
  { id: "advanced", label: "Troubleshooting", icon: Wrench },
  { id: "license", label: "License", icon: BadgeCheck },
  { id: "about", label: "About & updates", icon: Info },
] as const;

const paneDescriptions: Record<SettingsPane, string> = {
  general: "Appearance, startup and updates",
  shortcuts: "Extra shortcuts for app actions. Your dictation shortcut lives in Recording.",
  privacy: `Everything you dictate stays on this ${isMacOS ? "Mac" : "PC"} unless you choose a cloud engine or AI provider.`,
  storage: "Where your history and recordings live, and how long they stay.",
  network: "Let your other computers use this Voicetypr to transcribe. Local network only.",
  agent: "Use Voicetypr from the terminal and from your AI agents.",
  advanced: "Permissions, quick fixes and reset options.",
  license: "Trial status, license activation, and purchase access.",
  about: "Your installed version, updates and help.",
};

export function SettingsTab({
  pane,
  onPaneChange,
  onNavigate,
  modal = false,
}: {
  modal?: boolean;
  pane: SettingsPane;
  onPaneChange: (pane: SettingsPane) => void;
  onNavigate?: (screen: ScreenId) => void;
}) {
  return (
    <div className="settings-modal-layout flex h-full min-h-0">
      <nav
        aria-label="Settings panes"
        data-pencil-name="Nav"
        className="w-[212px] shrink-0 overflow-y-auto border-r border-border bg-sidebar px-2.5 py-[18px]"
      >
        <h1 className="px-2.5 pb-3 text-[15px] leading-[18px] font-semibold">Settings</h1>
        <div className="flex flex-col gap-0.5">
          {settingsPanes.map((entry, index) => {
            const Icon = entry.icon;
            return (
              <div key={entry.id}>
                {index === 4 || index === 7 ? (
                  <p className="px-2.5 pt-[14px] pb-1 text-[10.5px] leading-[13px] font-semibold uppercase tracking-[0.6px] text-text-3">
                    {index === 4 ? "Advanced" : "Account"}
                  </p>
                ) : null}
                <button
                  data-pencil-name={`Tab ${entry.label}`}
                  type="button"
                  onClick={() => onPaneChange(entry.id)}
                  aria-current={pane === entry.id ? "page" : undefined}
                  className={cn(
                    "flex w-full items-center gap-[9px] rounded-lg px-2.5 py-[7px] text-left text-[13px] leading-[17px] font-medium text-muted-foreground hover:bg-muted",
                    pane === entry.id &&
                      "bg-card font-semibold text-foreground shadow-[0_1px_2px_#0000000f]",
                  )}
                >
                  <Icon
                    aria-hidden="true"
                    className={cn("size-[15px] shrink-0", pane === entry.id && "text-sage")}
                  />
                  {entry.label}
                </button>
              </div>
            );
          })}
        </div>
      </nav>
      <div
        data-pencil-name="Content"
        className="flex min-w-0 flex-1 flex-col gap-[14px] pt-[18px] pr-[18px] pb-6 pl-7"
      >
        <header
          data-pencil-name="Head"
          className="flex shrink-0 items-center justify-between gap-3"
        >
          <div className="pt-1">
            <h2
              id="settings-pane-title"
              className="text-[18px] leading-[22px] font-semibold tracking-[-0.3px]"
            >
              {settingsPanes.find((entry) => entry.id === pane)?.label}
            </h2>
            <p
              id="settings-pane-description"
              className="mt-0.5 text-[12.5px] leading-[16px] text-muted-foreground"
            >
              {paneDescriptions[pane]}
            </p>
          </div>
          {modal ? (
            <div className="flex items-center gap-2">
              <kbd className="rounded-[5px] bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
                esc
              </kbd>
              <DialogClose
                aria-label="Close Settings"
                className="flex size-7 items-center justify-center rounded-lg bg-muted text-muted-foreground hover:text-foreground"
              >
                <X className="size-[18px]" />
              </DialogClose>
            </div>
          ) : null}
        </header>
        <div
          key={pane}
          data-pencil-name="Pane content"
          data-settings-pane={pane}
          className="settings-modal-pane flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto [&_[data-slot=select-trigger]]:rounded-[10px] [&_[data-slot=input]]:rounded-[10px]"
        >
          {pane === "general" ? (
            <>
              <GeneralSettings embedded />
            </>
          ) : null}
          {pane === "shortcuts" ? (
            <>
              <ShortcutsSection onNavigateRecording={() => onNavigate?.("recording")} />
            </>
          ) : null}
          {pane === "privacy" ? (
            <>
              <TelemetrySection />
            </>
          ) : null}
          {pane === "storage" ? (
            <>
              <SettingsPaneCard title="History">
                <StorageCleanupCard pane />
              </SettingsPaneCard>
              <StorageFilesCard onNavigate={onNavigate} />
            </>
          ) : null}
          {pane === "network" ? (
            <>
              <NetworkSharingCard pane />
            </>
          ) : null}
          {pane === "agent" ? (
            <>
              <AgentCliSection />
            </>
          ) : null}
          {pane === "advanced" ? (
            <div className="[&>div>header]:hidden">
              <AdvancedSection embedded />
            </div>
          ) : null}
          {pane === "license" ? <AccountTab embedded /> : null}
          {pane === "about" ? <AboutCard onNavigate={onNavigate} /> : null}
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

function AboutCard({ onNavigate }: { onNavigate?: (screen: ScreenId) => void }) {
  const [whatsNewOpen, setWhatsNewOpen] = useState(false);
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
    <>
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
        <SettingsPaneRow
          title="What's new"
          description="See what's changed in your installed version."
          control={
            <Button
              variant="outline"
              size="sm"
              disabled={version === "—"}
              onClick={() => setWhatsNewOpen(true)}
            >
              What's new
            </Button>
          }
        />
        <SettingsPaneRow
          title="Help & feedback"
          description="Get help or report a problem."
          control={
            <Button variant="outline" size="sm" onClick={() => onNavigate?.("help")}>
              Help & feedback
            </Button>
          }
        />
      </SettingsPaneCard>
      <UpdateAnnouncementDialog
        version={whatsNewOpen ? version : null}
        onClose={() => setWhatsNewOpen(false)}
      />
    </>
  );
}
