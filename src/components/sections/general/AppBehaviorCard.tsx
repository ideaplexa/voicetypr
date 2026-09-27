import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldTitle,
} from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { Switch } from "@/components/ui/switch";
import { createLogger } from "@/lib/logger";
import { updateService } from "@/services/updateService";
import type { UpdateChannel } from "@/types";
import { isStoreDistribution, type DistributionInfo } from "@/types/distribution";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw, Rocket } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

interface AppBehaviorCardProps {
  updateChannel: string | null | undefined;
  checkUpdatesAutomatically: boolean | null | undefined;
  onUpdateChannelChange: (channel: UpdateChannel) => Promise<void>;
  onCheckUpdatesAutomaticallyChange: (checked: boolean) => void;
  onLaunchAtStartupResolved: (enabled: boolean) => Promise<void> | void;
}
export function AppBehaviorCard({
  updateChannel,
  checkUpdatesAutomatically,
  onUpdateChannelChange,
  onCheckUpdatesAutomaticallyChange,
  onLaunchAtStartupResolved,
}: AppBehaviorCardProps) {
  const log = createLogger("settings");
  const [autostartEnabled, setAutostartEnabled] = useState(false);
  const [autostartLoading, setAutostartLoading] = useState(false);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [updateDistribution, setUpdateDistribution] = useState<
    "loading" | "direct" | "store" | "error"
  >("loading");
  const [isChangingUpdateChannel, setIsChangingUpdateChannel] = useState(false);

  useEffect(() => {
    const checkAutostart = async () => {
      try {
        const enabled = await invoke<boolean>("get_autostart_status");
        setAutostartEnabled(enabled);
      } catch (error) {
        log.error("Failed to check autostart status:", error);
      }
    };

    void checkAutostart();
  }, []);

  useEffect(() => {
    invoke<DistributionInfo>("get_distribution_info")
      .then((info) => setUpdateDistribution(isStoreDistribution(info) ? "store" : "direct"))
      .catch((error) => {
        log.error("Failed to check update distribution:", error);
        setUpdateDistribution("error");
      });
  }, []);

  const handleAutostartToggle = async (checked: boolean) => {
    setAutostartLoading(true);
    try {
      const actualState = await invoke<boolean>("set_autostart", {
        enabled: checked,
      });
      setAutostartEnabled(actualState);
      await onLaunchAtStartupResolved(actualState);

      if (actualState !== checked) {
        toast.warning(
          `Autostart ${checked ? "enable" : "disable"} failed. Current state: ${actualState ? "enabled" : "disabled"}.`,
        );
      }
    } catch (error) {
      log.error("Failed to toggle autostart:", error);
    } finally {
      setAutostartLoading(false);
    }
  };

  const handleCheckUpdate = async () => {
    setIsCheckingUpdate(true);
    try {
      await updateService.checkForUpdatesManually();
    } finally {
      setIsCheckingUpdate(false);
    }
  };

  const handleUpdateChannelChange = async (value: string) => {
    if (value !== "stable" && value !== "beta") {
      toast.error("Invalid update channel");
      return;
    }

    setIsChangingUpdateChannel(true);
    try {
      const channel: UpdateChannel = value;
      await onUpdateChannelChange(channel);
      toast.success(channel === "beta" ? "Beta updates enabled" : "Stable updates enabled", {
        description:
          channel === "beta"
            ? "Checking for the latest beta. Beta builds may be less stable."
            : "Future checks use stable releases. Switching does not downgrade an installed beta.",
      });
      await handleCheckUpdate();
    } catch (error) {
      log.error("Failed to change update channel:", error);
      toast.error("Failed to change update channel");
    } finally {
      setIsChangingUpdateChannel(false);
    }
  };

  return (
    <div className="rounded-2xl border border-border bg-card p-4">
      <div className="mb-4 flex items-center gap-2">
        <div className="rounded-md bg-sage-bg p-1.5">
          <Rocket className="h-4 w-4 text-sage" />
        </div>
        <div>
          <h3 className="font-medium">App behavior</h3>
          <p className="text-xs text-muted-foreground">Startup and update defaults</p>
        </div>
      </div>
      <FieldGroup className="gap-4">
        <Field orientation="responsive" className="items-center gap-4">
          <FieldContent>
            <FieldTitle>Launch at Startup</FieldTitle>
            <FieldDescription>Start Voicetypr automatically after login.</FieldDescription>
          </FieldContent>
          <div className="flex items-center justify-end gap-2">
            {autostartLoading && <Spinner className="h-4 w-4 text-muted-foreground" />}
            <Switch
              id="autostart"
              checked={autostartEnabled}
              onCheckedChange={handleAutostartToggle}
              disabled={autostartLoading}
            />
          </div>
        </Field>

        {updateDistribution === "store" ? (
          <Field>
            <FieldContent>
              <FieldTitle>Updates managed by Microsoft Store</FieldTitle>
              <FieldDescription>
                Update channels and installation are controlled by Microsoft Store.
              </FieldDescription>
            </FieldContent>
          </Field>
        ) : updateDistribution === "direct" ? (
          <>
            <Field orientation="responsive" className="items-center gap-4">
              <FieldContent>
                <FieldTitle>Update channel</FieldTitle>
                <FieldDescription>
                  Beta gets early builds for testing. Switching to Stable changes future checks and
                  does not downgrade an installed beta.
                </FieldDescription>
              </FieldContent>
              <Select
                items={[
                  { value: "stable", label: "Stable" },
                  { value: "beta", label: "Beta" },
                ]}
                value={updateChannel ?? "stable"}
                onValueChange={(value) => value != null && void handleUpdateChannelChange(value)}
                disabled={isChangingUpdateChannel || isCheckingUpdate}
              >
                <SelectTrigger className="w-full md:w-[190px]" aria-label="Update channel">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="stable">Stable</SelectItem>
                  <SelectItem value="beta">Beta</SelectItem>
                </SelectContent>
              </Select>
            </Field>

            <Field orientation="responsive" className="items-center gap-4">
              <FieldContent>
                <FieldTitle>Check for updates automatically</FieldTitle>
                <FieldDescription>
                  Check the selected channel daily and ask before downloading or installing
                  anything.
                </FieldDescription>
              </FieldContent>
              <div className="flex flex-col items-end gap-2">
                <Switch
                  id="check-updates-automatically"
                  checked={checkUpdatesAutomatically ?? true}
                  onCheckedChange={onCheckUpdatesAutomaticallyChange}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={handleCheckUpdate}
                  disabled={isCheckingUpdate}
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${isCheckingUpdate ? "animate-spin" : ""}`} />
                  {isCheckingUpdate ? "Checking" : "Check updates"}
                </Button>
              </div>
            </Field>
          </>
        ) : (
          <Field>
            <FieldContent>
              <FieldTitle>
                {updateDistribution === "loading"
                  ? "Loading update options"
                  : "Update options unavailable"}
              </FieldTitle>
              <FieldDescription>
                {updateDistribution === "loading"
                  ? "Checking how this installation receives updates."
                  : "Could not verify this installation type. Restart Voicetypr to try again."}
              </FieldDescription>
            </FieldContent>
          </Field>
        )}
      </FieldGroup>
    </div>
  );
}
