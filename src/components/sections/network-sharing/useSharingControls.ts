import { invoke } from "@tauri-apps/api/core";
import type { Dispatch, SetStateAction } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";
import { MAX_SHARING_PORT, MIN_SHARING_PORT, parseSharingPort } from "./sharingUtils";
import type { SharingStatus } from "./types";
import type { AppSettings } from "@/types";

const log = createLogger("network");

interface SharingControlsInput {
  status: SharingStatus;
  setStatus: Dispatch<SetStateAction<SharingStatus>>;
  port: string;
  setPort: (port: string) => void;
  password: string;
  setPassword: (password: string) => void;
  savedPort: string;
  setSavedPort: (port: string) => void;
  savedPassword: string;
  setSavedPassword: (password: string) => void;
  setLoading: (loading: boolean) => void;
  setSavingPort: (saving: boolean) => void;
  setSavingPassword: (saving: boolean) => void;
  setSavingModelControl: (saving: boolean) => void;
  fetchStatus: () => Promise<void>;
  fetchFirewallStatus: () => Promise<void>;
  updateSettings: (partial: Partial<AppSettings>) => Promise<void>;
}

export function useSharingControls({
  status,
  setStatus,
  port,
  setPort,
  password,
  setPassword,
  savedPort,
  setSavedPort,
  savedPassword,
  setSavedPassword,
  setLoading,
  setSavingPort,
  setSavingPassword,
  setSavingModelControl,
  fetchStatus,
  fetchFirewallStatus,
  updateSettings,
}: SharingControlsInput) {
  const handleToggleModelControl = async (checked: boolean) => {
    setSavingModelControl(true);
    try {
      await invoke("update_remote_model_control_enabled", { enabled: checked });
      setStatus((current) => ({ ...current, allow_model_control: checked }));
      toast.success(
        checked ? "Host model changes enabled for trusted devices" : "Host model changes disabled",
      );
    } catch (error) {
      log.error("Failed to update remote model control setting:", error);
      toast.error("Failed to update remote model control setting");
      await fetchStatus();
    } finally {
      setSavingModelControl(false);
    }
  };

  const handleToggleSharing = async (checked: boolean) => {
    setLoading(true);
    try {
      if (checked) {
        const validatedPort = parseSharingPort(port);
        if (!validatedPort) {
          toast.error(`Enter a valid port between ${MIN_SHARING_PORT} and ${MAX_SHARING_PORT}`);
          return;
        }

        await invoke("start_sharing", {
          port: validatedPort,
          password: password || null,
          preservePassword: !password,
          serverName: null, // Use hostname
        });
        toast.success("Remote transcription enabled");
        fetchFirewallStatus();
      } else {
        await invoke("stop_sharing");
        toast.success("Remote transcription disabled");
      }
      await fetchStatus();
    } catch (error) {
      log.error("Failed to toggle sharing:", error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      toast.error(errorMessage || "Failed to toggle remote transcription");
      await fetchStatus();
    } finally {
      setLoading(false);
    }
  };

  const restorePreviousSharing = async (): Promise<boolean> => {
    const previousPort = parseSharingPort(savedPort);
    if (!previousPort) return false;

    await invoke("start_sharing", {
      port: previousPort,
      password: savedPassword || null,
      preservePassword: !savedPassword,
      serverName: null,
    });
    return true;
  };

  const handleSavePort = async () => {
    if (!status.enabled) return;

    const validatedPort = parseSharingPort(port);
    if (!validatedPort) {
      toast.error(`Enter a valid port between ${MIN_SHARING_PORT} and ${MAX_SHARING_PORT}`);
      return;
    }

    setSavingPort(true);
    try {
      await invoke("stop_sharing");
      await invoke("start_sharing", {
        port: validatedPort,
        password: null,
        preservePassword: true,
        serverName: null,
      });
      setSavedPort(port);
      await updateSettings({ sharing_port: validatedPort });
      await fetchStatus();
      toast.success(`Port changed to ${port}`);
    } catch (error) {
      log.error("Failed to update port:", error);
      setPort(savedPort);
      try {
        await restorePreviousSharing();
        toast.error("Failed to update port; sharing restored");
      } catch (restoreError) {
        log.error("Failed to restore sharing after port change:", restoreError);
        toast.error("Failed to update port and could not restore sharing");
      }
      await fetchStatus();
    } finally {
      setSavingPort(false);
    }
  };

  const handleSavePassword = async () => {
    if (!status.enabled) return;

    const validatedPort = parseSharingPort(savedPort);
    if (!validatedPort) {
      toast.error(`Enter a valid port between ${MIN_SHARING_PORT} and ${MAX_SHARING_PORT}`);
      return;
    }

    let disabledModelControl = false;

    setSavingPassword(true);
    try {
      if (!password && status.allow_model_control) {
        await invoke("update_remote_model_control_enabled", { enabled: false });
        disabledModelControl = true;
      }
      await invoke("stop_sharing");
      await invoke("start_sharing", {
        port: validatedPort,
        password: password || null,
        preservePassword: false,
        serverName: null,
      });
      if (disabledModelControl) {
        setStatus((current) => ({ ...current, allow_model_control: false }));
      }
      setPassword("");
      setSavedPassword("");
      await fetchStatus();
      toast.success(password ? "Password updated" : "Password removed");
    } catch (error) {
      log.error("Failed to update password:", error);
      setPassword(savedPassword);
      try {
        await restorePreviousSharing();
      } catch (restoreError) {
        log.error("Failed to restore sharing after password change:", restoreError);
        toast.error("Failed to update password and could not restore sharing");
        await fetchStatus();
        return;
      }

      if (disabledModelControl) {
        try {
          await invoke("update_remote_model_control_enabled", { enabled: true });
          setStatus((current) => ({ ...current, allow_model_control: true }));
        } catch (restoreModelControlError) {
          log.error("Failed to restore remote model control:", restoreModelControlError);
          toast.error("Failed to update password; sharing restored without remote model changes");
          await fetchStatus();
          return;
        }
      }

      toast.error("Failed to update password; sharing restored");
      await fetchStatus();
    } finally {
      setSavingPassword(false);
    }
  };

  const copyAddress = (ip: string) => {
    // Extract just the IP from "192.168.1.1 (eth0)" format
    const justIp = ip.split(" ")[0];
    const address = `${justIp}:${savedPort}`;
    navigator.clipboard.writeText(address);
    toast.success("Address copied to clipboard");
  };

  return {
    handleToggleModelControl,
    handleToggleSharing,
    handleSavePort,
    handleSavePassword,
    copyAddress,
  };
}
