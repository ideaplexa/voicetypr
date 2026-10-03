import { useEffect, useState, type CSSProperties } from "react";
import { CircleAlert, PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { toast } from "sonner";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import { Sidebar } from "@/components/Sidebar";
import { TabContainer } from "@/components/tabs/TabContainer";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { SidebarInset, SidebarProvider, SidebarTrigger, useSidebar } from "@/components/ui/sidebar";
import { Spinner } from "@/components/ui/spinner";
import { createLogger } from "@/lib/logger";
import { isMacOS } from "@/lib/platform";
import { getTrayStatus, retryTrayCreation, type TrayStatus } from "@/lib/tray";

const log = createLogger("app-shell");

import type { SourceFilterProps } from "@/components/sections/models/types";

interface AppShellProps extends SourceFilterProps {
  activeSection: ScreenId;
  onSectionChange: (section: ScreenId) => void;
  settingsPane?: SettingsPane;
  onSettingsClose?: () => void;
  onSettingsPaneChange?: (pane: SettingsPane) => void;
}

export function AppShell({
  activeSection,
  onSectionChange,
  settingsPane,
  onSettingsPaneChange,
  onSettingsClose,
  ...sourceFilterProps
}: AppShellProps) {
  const [trayStatus, setTrayStatus] = useState<TrayStatus | null>(null);
  const [isRetryingTray, setIsRetryingTray] = useState(false);
  useEffect(() => {
    void getTrayStatus()
      .then(setTrayStatus)
      .catch((error) => {
        log.warn("Failed to read tray status:", error);
      });
  }, []);

  useTauriEvent<TrayStatus>("tray-status-changed", setTrayStatus);

  const handleRetryTray = async () => {
    setIsRetryingTray(true);
    try {
      const status = await retryTrayCreation();
      setTrayStatus(status);
      if (status.available) {
        toast.success("Menu-bar icon restored");
      } else {
        toast.error(
          "Menu-bar icon is still unavailable. Keep this window open and report the issue.",
        );
      }
    } catch (error) {
      log.error("Failed to retry tray creation:", error);
      toast.error("Could not retry the menu-bar icon. Keep this window open and report the issue.");
    } finally {
      setIsRetryingTray(false);
    }
  };

  const trayUnavailable = trayStatus !== null && !trayStatus.available && trayStatus.attempts > 0;

  return (
    <SidebarProvider
      className="bg-background"
      defaultOpen={!document.cookie.split("; ").includes("sidebar_state=false")}
      style={
        {
          "--sidebar-width": "212px",
          "--sidebar-width-icon": "76px",
        } as CSSProperties
      }
    >
      <ShellTitleStrip />
      <Sidebar activeSection={activeSection} onSectionChange={onSectionChange} />
      <SidebarInset className="m-0 h-svh min-h-0 min-w-0 overflow-hidden rounded-none bg-background">
        {trayUnavailable ? (
          <Alert variant="destructive" className="mx-4 mt-4">
            <CircleAlert />
            <AlertTitle>Menu-bar icon unavailable</AlertTitle>
            <AlertDescription>
              Voicetypr could not create its menu-bar icon after {trayStatus.attempts} attempts.
              Keep this window open, then retry or submit a bug report with the included diagnostic.
            </AlertDescription>
            <AlertAction>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={isRetryingTray}
                onClick={handleRetryTray}
              >
                {isRetryingTray ? <Spinner data-icon="inline-start" /> : null}
                Retry icon
              </Button>
            </AlertAction>
          </Alert>
        ) : null}
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
          <TabContainer
            activeSection={activeSection}
            onNavigate={onSectionChange}
            settingsPane={settingsPane}
            onSettingsPaneChange={onSettingsPaneChange}
            onSettingsClose={onSettingsClose}
            {...sourceFilterProps}
          />
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}

/** Kept outside the inset so collapsing cannot move the window control. */
function ShellTitleStrip() {
  const { state } = useSidebar();
  const Icon = state === "expanded" ? PanelLeftClose : PanelLeftOpen;
  return (
    <header data-tauri-drag-region className="fixed inset-x-0 top-0 z-50 h-9">
      <span className={`absolute top-[5px] size-7 ${isMacOS ? "left-[80px]" : "left-6"}`}>
        <SidebarTrigger
          title="Toggle sidebar"
          className={`size-7 rounded-[7px] text-muted-foreground [&>svg]:hidden ${state === "collapsed" ? "bg-foreground/[0.04]" : ""}`}
        />
        <Icon
          aria-hidden="true"
          className="pointer-events-none absolute left-[6px] top-[6px] size-4 text-muted-foreground"
        />
      </span>
    </header>
  );
}
