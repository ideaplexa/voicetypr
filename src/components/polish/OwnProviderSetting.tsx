import { SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/settings/SettingsSwitch";

/** The user's own AI provider for Polish, the second way to get it after built-in Polish. */
export function OwnProviderSetting({ enabled, hasSelectedModel, providerName, onCheckedChange, onOpenProviderSetup }: {
  enabled: boolean;
  hasSelectedModel: boolean;
  providerName: string;
  onCheckedChange: (enabled: boolean) => void;
  onOpenProviderSetup: () => void;
}) {
  return <SettingRow title="Clean up with my own AI provider" htmlFor="polish-enabled"
    description={!hasSelectedModel
      ? enabled ? "Needs a model before it can polish. Use the switch to finish setup." : "No provider set up yet: turn this on to choose one."
      : enabled ? `Uses ${providerName} instead of Voicetypr’s server.`
      : `Turn on to polish with ${providerName} instead of Voicetypr’s server.`}
    control={<Switch id="polish-enabled" checked={enabled} onCheckedChange={hasSelectedModel ? onCheckedChange : onOpenProviderSetup} />} />;
}
