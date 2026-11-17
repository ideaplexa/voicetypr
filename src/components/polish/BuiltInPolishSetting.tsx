import { SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/settings/SettingsSwitch";

/** Built-in Polish: Voicetypr's server cleans up dictation for users without their own provider. */
export function BuiltInPolishSetting({ checked, ownProvider, entitled, onCheckedChange }: {
  checked: boolean;
  /** The user's own provider is set up, so it is used instead. */
  ownProvider: boolean;
  /** An active trial or a licence. */
  entitled: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  const status = ownProvider
    ? "Not used: your own provider polishes your dictation."
    : !entitled
      ? "Works during your trial or with a license."
      : checked
        ? "On: used because you haven’t connected your own provider."
        : "Off: built-in Polish won’t send your text.";
  return <SettingRow title="Clean up on Voicetypr’s server" htmlFor="builtin-polish-enabled"
    description={<>
      Voicetypr’s built-in Polish, for when you haven’t connected an AI of your own. Your dictated text is sent to
      Voicetypr’s server and Cloudflare Workers AI to do this, and is not stored. It doesn’t translate. If it
      can’t answer in time, Voicetypr pastes your transcript with your dictionary corrections, as it does with Polish off.
      <span className="mt-1 block" role="status">{status}</span>
    </>}
    control={<Switch id="builtin-polish-enabled" checked={checked} disabled={ownProvider} onCheckedChange={onCheckedChange} />} />;
}
