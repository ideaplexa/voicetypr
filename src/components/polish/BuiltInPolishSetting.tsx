import { SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/settings/SettingsSwitch";
import type { BuiltinPolishAvailability } from "@/types";

/**
 * What the built-in Polish switch shows, the same in onboarding and Settings: the switch is the
 * user's saved choice and can always be turned off; whether it runs now is the status line.
 * `active` is true only when it is chosen and Voicetypr's server confirmed it will run.
 */
export function builtinPolishView(availability: BuiltinPolishAvailability | null, chosen: boolean) {
  const status = availability === "own_provider" ? "Not used: you’ve set up your own AI provider."
    : !chosen ? "Off: built-in Polish won’t send your text."
    : availability === null ? "Checking with Voicetypr’s server…"
    : availability === "no_access" || availability === "first_try" ? "Works during your trial or with a license."
    : availability === "not_entitled"
      ? "Not available on this device: built-in Polish needs a trial started with your email, or a license."
    : availability === "unreachable"
      ? "Voicetypr’s server can’t take Polish right now (offline or busy), so nothing is sent and you get exactly what you said."
    : "On: used because you haven’t connected your own provider.";
  return { checked: chosen, active: chosen && availability === "ready", status };
}

/** Built-in Polish: Voicetypr's server cleans up dictation for users without their own provider. */
export function BuiltInPolishSetting({ chosen, availability, onCheckedChange }: {
  /** The saved `builtin_polish_enabled` choice. */
  chosen: boolean;
  /** From `polish_status`; null while it is being asked. */
  availability: BuiltinPolishAvailability | null;
  onCheckedChange: (checked: boolean) => void;
}) {
  const view = builtinPolishView(availability, chosen);
  return <SettingRow title="Clean up on Voicetypr’s server" htmlFor="builtin-polish-enabled"
    description={<>
      Voicetypr’s built-in Polish, for when you haven’t connected an AI of your own. Your dictated text is sent to
      Voicetypr’s server and Cloudflare Workers AI to do this, and is not stored. It doesn’t translate. If it
      can’t answer in time, Voicetypr pastes your transcript with your dictionary corrections, as it does with Polish off.
      <span className="mt-1 block" role="status">{view.status}</span>
    </>}
    control={<Switch id="builtin-polish-enabled" checked={view.checked} onCheckedChange={onCheckedChange} />} />;
}
