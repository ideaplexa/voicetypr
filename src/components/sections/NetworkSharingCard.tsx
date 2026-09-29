import { SharingControls } from "./network-sharing/SharingControls";
import { SharingStatusHeader } from "./network-sharing/SharingStatusHeader";
import { useSharingControls } from "./network-sharing/useSharingControls";
import { useSharingStatus } from "./network-sharing/useSharingStatus";
import { BindingResultsList } from "./network-sharing/BindingResultsList";
import { ConnectionSettingsPanel } from "./network-sharing/ConnectionSettingsPanel";
import { SettingsPaneCard, SettingsPaneRow } from "@/components/settings/settings-ui";
import { Button } from "@/components/ui/button";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";
import { isMacOS, isWindows } from "@/lib/platform";

const log = createLogger("network");

export function NetworkSharingCard({ pane = false }: { pane?: boolean } = {}) {
  const sharing = useSharingStatus();
  const actions = useSharingControls({
    status: sharing.status,
    setStatus: sharing.setStatus,
    port: sharing.port,
    setPort: sharing.setPort,
    password: sharing.password,
    setPassword: sharing.setPassword,
    savedPort: sharing.savedPort,
    setSavedPort: sharing.setSavedPort,
    savedPassword: sharing.savedPassword,
    setSavedPassword: sharing.setSavedPassword,
    setLoading: sharing.setLoading,
    setSavingPort: sharing.setSavingPort,
    setSavingPassword: sharing.setSavingPassword,
    setSavingModelControl: sharing.setSavingModelControl,
    fetchStatus: sharing.fetchStatus,
    fetchFirewallStatus: sharing.fetchFirewallStatus,
    updateSettings: sharing.updateSettings,
  });

  if (pane)
    return (
      <div className="flex flex-col gap-3">
        <SettingsPaneCard>
          <SharingStatusHeader
            pane
            enabled={sharing.status.enabled}
            loading={sharing.loading}
            activeRemoteServer={sharing.activeRemoteServer}
            hasShareableModel={sharing.hasShareableModel}
            currentSelectionShareable={sharing.currentSelectionShareable}
            modelDisplayName={sharing.modelDisplayName}
            onToggleSharing={(checked) => {
              void actions.handleToggleSharing(checked);
            }}
          />
          {sharing.status.enabled ? (
            <>
              <SettingsPaneRow
                title="Address"
                description="Give this to your other computer."
                control={
                  <BindingResultsList
                    pane
                    bindingResults={sharing.status.binding_results}
                    savedPort={sharing.savedPort}
                    onCopyAddress={actions.copyAddress}
                  />
                }
              />
              <ConnectionSettingsPanel
                pane
                enabled={sharing.status.enabled}
                port={sharing.port}
                savedPort={sharing.savedPort}
                password={sharing.password}
                savedPassword={sharing.savedPassword}
                showPassword={sharing.showPassword}
                savingPort={sharing.savingPort}
                savingPassword={sharing.savingPassword}
                savingModelControl={sharing.savingModelControl}
                passwordConfigured={sharing.status.password_configured}
                allowModelControl={sharing.status.allow_model_control}
                onPortChange={sharing.setPort}
                onPasswordChange={sharing.setPassword}
                onTogglePasswordVisibility={() => sharing.setShowPassword(!sharing.showPassword)}
                onSavePort={() => {
                  void actions.handleSavePort();
                }}
                onSavePassword={() => {
                  void actions.handleSavePassword();
                }}
                onToggleModelControl={(checked) => {
                  void actions.handleToggleModelControl(checked);
                }}
              />
            </>
          ) : null}
        </SettingsPaneCard>
        {sharing.status.enabled ? (
          <SettingsPaneCard title="Status">
            <SettingsPaneRow
              title="Firewall"
              description={
                sharing.firewallStatus?.may_be_blocked
                  ? "Firewall may block connections"
                  : "Incoming connections are allowed."
              }
              control={
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
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
                  >
                    Open
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      toast.info("Checking firewall status...");
                      void sharing.fetchFirewallStatus();
                    }}
                  >
                    Check again
                  </Button>
                </div>
              }
            />
            <SettingsPaneRow
              title="Connected devices"
              description={
                sharing.status.active_connections === 1
                  ? "One device connected"
                  : `${sharing.status.active_connections} devices connected`
              }
              control={
                <span className="text-sm font-medium text-foreground">
                  {sharing.status.active_connections}
                </span>
              }
            />
          </SettingsPaneCard>
        ) : null}
      </div>
    );

  return (
    <div className="rounded-lg border border-border/50 bg-card">
      <SharingStatusHeader
        enabled={sharing.status.enabled}
        loading={sharing.loading}
        activeRemoteServer={sharing.activeRemoteServer}
        hasShareableModel={sharing.hasShareableModel}
        currentSelectionShareable={sharing.currentSelectionShareable}
        modelDisplayName={sharing.modelDisplayName}
        onToggleSharing={(checked) => {
          void actions.handleToggleSharing(checked);
        }}
      />

      {sharing.status.enabled && (
        <SharingControls
          status={sharing.status}
          firewallStatus={sharing.firewallStatus}
          sharedModelDisplayName={sharing.sharedModelDisplayName}
          port={sharing.port}
          savedPort={sharing.savedPort}
          password={sharing.password}
          savedPassword={sharing.savedPassword}
          showPassword={sharing.showPassword}
          savingPort={sharing.savingPort}
          savingPassword={sharing.savingPassword}
          savingModelControl={sharing.savingModelControl}
          onPortChange={sharing.setPort}
          onPasswordChange={sharing.setPassword}
          onTogglePasswordVisibility={() => sharing.setShowPassword(!sharing.showPassword)}
          onSavePort={() => {
            void actions.handleSavePort();
          }}
          onSavePassword={() => {
            void actions.handleSavePassword();
          }}
          onToggleModelControl={(checked) => {
            void actions.handleToggleModelControl(checked);
          }}
          onCopyAddress={actions.copyAddress}
          onRecheckFirewall={sharing.fetchFirewallStatus}
        />
      )}
    </div>
  );
}
