import { SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/settings/SettingsSwitch";

/** The user's own AI provider for Polish, the second way to get it after built-in Polish. */
export function OwnProviderSetting({ enabled, hasSelectedModel, providerName, modelName, onCheckedChange, onOpenProviderSetup }: {
  enabled: boolean;
  hasSelectedModel: boolean;
  providerName: string;
  modelName: string;
  onCheckedChange: (enabled: boolean) => void;
  onOpenProviderSetup: () => void;
}) {
  const provider = `${providerName}${modelName ? ` · ${modelName}` : ""}`;
  return <SettingRow title="Clean up with my own AI provider" htmlFor="polish-enabled"
    description={!hasSelectedModel ? "No provider set up yet: turn this on to choose one."
      : enabled ? `Uses ${provider} instead of Voicetypr’s server.`
      : `${provider} is set up. Turn this on to use it instead of Voicetypr’s server.`}
    control={<Switch id="polish-enabled" checked={enabled} onCheckedChange={hasSelectedModel ? onCheckedChange : onOpenProviderSetup} />} />;
}
