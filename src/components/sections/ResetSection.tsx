import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import { Button } from "@/components/ui/button";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { Loader2, RefreshCw, RotateCcw, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

const log = createLogger("reset-settings");

export function ResetSection() {
  const { updateSettings } = useSettings();
  const [isResetting, setIsResetting] = useState(false);

  const handleResetOnboarding = async () => {
    try {
      await updateSettings({ onboarding_completed: false });
      toast.success("Onboarding reset!");
    } catch (error) {
      log.error("Failed to reset onboarding:", error);
      toast.error("Failed to reset onboarding");
    }
  };

  const handleResetAppData = async () => {
    const confirmed = await ask(
      "This action cannot be undone. This will permanently delete all your Voicetypr data.\n\nThe app will restart after reset.\n\nAre you absolutely sure?",
      {
        title: "Reset App Data",
        okLabel: "Reset Everything",
        cancelLabel: "Cancel",
        kind: "warning",
      },
    );

    if (!confirmed) return;

    setIsResetting(true);
    try {
      await invoke("reset_app_data");
      toast.success("App data reset successfully. Restarting...");
      setTimeout(() => {
        void relaunch();
      }, 1000);
    } catch (error) {
      log.error("Failed to reset app data:", error);
      toast.error("Failed to reset app data");
      setIsResetting(false);
    }
  };

  return (
    <SettingsCard
      icon={RotateCcw}
      title="Reset app / start over"
      description="Re-run setup or wipe Voicetypr back to a clean state."
    >
      <SettingRow title="Reset Onboarding" description="Re-run the initial setup wizard">
        <Button variant="outline" size="sm" onClick={handleResetOnboarding}>
          <RefreshCw className="h-3 w-3" />
          Reset
        </Button>
      </SettingRow>

      <div className="mt-4 border-t border-border pt-4">
        <p className="mb-1 text-[13.5px] font-semibold text-foreground">Reset App Data</p>
        <p className="mb-2 text-[12.5px] text-muted-foreground">
          Completely reset Voicetypr to its initial state
        </p>
        <ul className="mb-3 list-inside list-disc space-y-0.5 text-xs text-muted-foreground">
          <li>Delete all transcription history</li>
          <li>Remove all downloaded models</li>
          <li>Clear all settings and preferences</li>
          <li>Reset system permissions</li>
        </ul>
        <Button
          variant="destructive"
          size="sm"
          disabled={isResetting}
          onClick={handleResetAppData}
          className="w-full"
        >
          {isResetting ? (
            <>
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              Resetting...
            </>
          ) : (
            <>
              <Trash2 className="mr-2 h-4 w-4" />
              Reset App Data
            </>
          )}
        </Button>
      </div>
    </SettingsCard>
  );
}
