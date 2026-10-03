import { SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/settings/SettingsSwitch";

export function KeepWordsSetting({ checked, onCheckedChange }: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return <SettingRow title="Keep my words" htmlFor="polish-keep-words"
    description="Only fix punctuation, fillers and stutters — never reword."
    control={<Switch id="polish-keep-words" checked={checked} onCheckedChange={onCheckedChange} />} />;
}
