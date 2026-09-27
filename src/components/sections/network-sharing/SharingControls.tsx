import { invoke } from "@tauri-apps/api/core";
import { isMacOS, isWindows } from "@/lib/platform";
import { ExternalLink, Server, Shield } from "lucide-react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";
import { BindingResultsList } from "./BindingResultsList";
import { ConnectionSettingsPanel } from "./ConnectionSettingsPanel";
import type { FirewallStatus, SharingStatus } from "./types";

const log = createLogger("network");

interface SharingControlsProps {
  status: SharingStatus;
  firewallStatus: FirewallStatus | null;
  sharedModelDisplayName: string | null;
  port: string;
  savedPort: string;
  password: string;
  savedPassword: string;
  showPassword: boolean;
  savingPort: boolean;
  savingPassword: boolean;
  savingModelControl: boolean;
  onPortChange: (value: string) => void;
  onPasswordChange: (value: string) => void;
  onTogglePasswordVisibility: () => void;
  onSavePort: () => void;
  onSavePassword: () => void;
  onToggleModelControl: (checked: boolean) => void;
  onCopyAddress: (ip: string) => void;
  onRecheckFirewall: () => void;
}

export function SharingControls({
  status,
  firewallStatus,
  sharedModelDisplayName,
  port,
  savedPort,
  password,
  savedPassword,
  showPassword,
  savingPort,
  savingPassword,
  savingModelControl,
  onPortChange,
  onPasswordChange,
  onTogglePasswordVisibility,
  onSavePort,
  onSavePassword,
  onToggleModelControl,
  onCopyAddress,
  onRecheckFirewall,
}: SharingControlsProps) {
  return (
    <div className="mx-3 mb-3 mt-0 rounded-lg bg-muted/20">
      <div className="p-4 space-y-4">
        <div className="flex items-center gap-2 p-3 rounded-lg bg-green-500/10 border border-green-500/20">
          <Server className="h-4 w-4 text-green-500" />
          <div className="flex-1">
            <p className="text-sm font-medium text-green-700 dark:text-green-400">
              Ready for remote transcription
            </p>
            <p className="text-xs text-muted-foreground">
              {sharedModelDisplayName ? `Model: ${sharedModelDisplayName}` : "No model selected"}
            </p>
          </div>
        </div>

        {firewallStatus?.may_be_blocked && (
          <div className="flex items-start gap-2 p-3 rounded-lg bg-amber-500/10 border border-amber-500/20">
            <Shield className="h-4 w-4 text-amber-500 mt-0.5 flex-shrink-0" />
            <div className="flex-1">
              <p className="text-sm font-medium text-amber-700 dark:text-amber-400">
                Firewall may block connections
              </p>
              {isMacOS && (
                <>
                  <p className="text-xs text-amber-600 dark:text-amber-500 mb-2">
                    Your macOS firewall is enabled. To allow other devices to connect:
                  </p>
                  <ol className="text-xs text-amber-600 dark:text-amber-500 mb-2 list-decimal list-inside space-y-0.5">
                    <li>
                      Open <strong>System Settings → Network → Firewall</strong>
                    </li>
                    <li>
                      Click <strong>Options...</strong>
                    </li>
                    <li>
                      Click the <strong>+</strong> button at the bottom of the app list
                    </li>
                    <li>
                      Navigate to <strong>Applications</strong> and select{" "}
                      <strong>Voicetypr</strong>
                    </li>
                    <li>
                      Ensure it's set to <strong>Allow incoming connections</strong>
                    </li>
                  </ol>
                </>
              )}
              {isWindows && (
                <>
                  <p className="text-xs text-amber-600 dark:text-amber-500 mb-2">
                    Windows Firewall may be blocking incoming connections. To allow other devices to
                    connect:
                  </p>
                  <ol className="text-xs text-amber-600 dark:text-amber-500 mb-2 list-decimal list-inside space-y-0.5">
                    <li>
                      Open <strong>Windows Firewall</strong> settings
                    </li>
                    <li>
                      Click <strong>Allow an app through firewall</strong>
                    </li>
                    <li>
                      Click <strong>Change settings</strong> (may require admin)
                    </li>
                    <li>
                      Click <strong>Allow another app...</strong>
                    </li>
                    <li>
                      Browse to and select <strong>Voicetypr</strong>
                    </li>
                    <li>
                      Check both <strong>Private</strong> and <strong>Public</strong> networks
                    </li>
                  </ol>
                </>
              )}
              {!isMacOS && !isWindows && (
                <p className="text-xs text-amber-600 dark:text-amber-500 mb-2">
                  Your firewall may be blocking incoming connections. Please configure your firewall
                  to allow Voicetypr.
                </p>
              )}
              <div className="flex items-center gap-3">
                <button
                  onClick={async () => {
                    try {
                      await invoke("open_firewall_settings");
                    } catch (error) {
                      log.error("Failed to open firewall settings:", error);
                      const settingsPath = isMacOS
                        ? "System Settings > Network > Firewall"
                        : isWindows
                          ? "Control Panel > Windows Firewall"
                          : "your firewall settings";
                      toast.error(
                        `Could not open Firewall settings. Please open ${settingsPath} manually.`,
                      );
                    }
                  }}
                  className="inline-flex items-center gap-1.5 text-xs font-medium text-amber-700 dark:text-amber-400 hover:underline"
                >
                  <ExternalLink className="h-3 w-3" />
                  {isMacOS
                    ? "Open System Settings"
                    : isWindows
                      ? "Open Windows Firewall"
                      : "Open Firewall Settings"}
                </button>
                <button
                  onClick={async () => {
                    toast.info("Checking firewall status...");
                    await onRecheckFirewall();
                  }}
                  className="text-xs text-amber-600 dark:text-amber-500 hover:underline"
                >
                  Check again
                </button>
              </div>
            </div>
          </div>
        )}

        <BindingResultsList
          bindingResults={status.binding_results}
          savedPort={savedPort}
          onCopyAddress={onCopyAddress}
        />

        <ConnectionSettingsPanel
          enabled={status.enabled}
          port={port}
          savedPort={savedPort}
          password={password}
          savedPassword={savedPassword}
          showPassword={showPassword}
          savingPort={savingPort}
          savingPassword={savingPassword}
          savingModelControl={savingModelControl}
          passwordConfigured={status.password_configured}
          allowModelControl={status.allow_model_control}
          onPortChange={onPortChange}
          onPasswordChange={onPasswordChange}
          onTogglePasswordVisibility={onTogglePasswordVisibility}
          onSavePort={onSavePort}
          onSavePassword={onSavePassword}
          onToggleModelControl={onToggleModelControl}
        />
      </div>
    </div>
  );
}
