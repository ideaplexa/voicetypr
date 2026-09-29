import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Button } from "@/components/ui/button";
import { SettingsCard } from "@/components/settings/settings-ui";
import type { SettingsPane } from "@/components/navigation";
import { updateService } from "@/services/updateService";
import { GeneralSettings } from "@/components/sections/GeneralSettings";
import { ShortcutsSection } from "@/components/sections/ShortcutsSection";
import { AdvancedSection } from "@/components/sections/AdvancedSection";
import { NetworkSharingTab } from "@/components/tabs/NetworkSharingTab";
import { AgentCliTab } from "@/components/tabs/AgentCliTab";
import { Keyboard, Settings2, Share2, Terminal, Wrench } from "lucide-react";
import { cn } from "@/lib/utils";

const panes = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "shortcuts", label: "Shortcuts", icon: Keyboard },
  { id: "network", label: "Network sharing", icon: Share2 },
  { id: "agent", label: "CLI & API", icon: Terminal },
  { id: "advanced", label: "Troubleshooting", icon: Wrench },
] as const;

export function SettingsTab({ pane, onPaneChange }: { pane: SettingsPane; onPaneChange: (pane: SettingsPane) => void }) {
  return (
    <div className="h-full min-h-0 overflow-auto px-5 py-5">
      <div className="mx-auto w-full max-w-5xl">
        <h1 className="mb-5 text-2xl font-semibold tracking-tight">Settings</h1>
        <div className="flex flex-col gap-6 md:flex-row">
          <nav
            aria-label="Settings panes"
            className="flex w-full shrink-0 flex-col gap-1 md:w-[170px]"
          >
            {panes.map((entry, index) => {
              const Icon = entry.icon;
              return (
                <div key={entry.id}>
                  {index === 2 ? (
                    <p className="px-2 pb-1 pt-5 text-[10px] font-semibold uppercase tracking-wider text-text-3">
                      Advanced
                    </p>
                  ) : null}
                  <button
                    type="button"
                    onClick={() => onPaneChange(entry.id)}
                    aria-current={pane === entry.id ? "page" : undefined}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-[10px] px-2 py-2 text-left text-[13px] text-muted-foreground hover:bg-muted",
                      pane === entry.id && "bg-muted font-semibold text-foreground",
                    )}
                  >
                    <Icon className={cn("size-4", pane === entry.id && "text-sage")} />
                    {entry.label}
                  </button>
                </div>
              );
            })}
          </nav>
          <div className="min-w-0 flex-1">
            {pane === "general" ? (
              <>
                <GeneralSettings embedded />
                <AboutCard />
              </>
            ) : null}
            {pane === "shortcuts" ? <ShortcutsSection /> : null}
            {pane === "network" ? <NetworkSharingTab /> : null}
            {pane === "agent" ? <AgentCliTab /> : null}
            {pane === "advanced" ? <AdvancedSection /> : null}
          </div>
        </div>
      </div>
    </div>
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
    <SettingsCard title="About" className="mt-4">
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-border pt-4">
        <span className="text-[13.5px] font-medium">Voicetypr {version}</span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => void checkUpdates()}
          disabled={checking}
        >
          Check for updates
        </Button>
      </div>
    </SettingsCard>
  );
}
