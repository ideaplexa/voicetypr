import { SharingControls } from "./network-sharing/SharingControls";
import { SharingStatusHeader } from "./network-sharing/SharingStatusHeader";
import { useSharingControls } from "./network-sharing/useSharingControls";
import { useSharingStatus } from "./network-sharing/useSharingStatus";

export function NetworkSharingCard() {
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
