import { FrameHeading, PrimaryButton, QuietButton } from "@/components/onboarding/Frame";
import { builtinPolishView } from "@/components/polish/BuiltInPolishSetting";
import { Switch } from "@/components/settings/SettingsSwitch";
import { isMacOS } from "@/lib/platform";
import type { PolishStatus } from "@/types";
import { Check, Settings2, ShieldCheck, Sparkles, Type } from "lucide-react";

/** 07 Let AI tidy your words (07b off): one switch, shared with Settings → Polish. */
export function PolishStep({
  status,
  builtinChosen,
  onBuiltinChange,
  saving,
  ownProviderLater,
  onOwnProvider,
  onNext,
}: {
  /** `polish_status`, the gate dictation uses; null while it is being asked. */
  status: PolishStatus | null;
  /** The saved built-in Polish choice, shared with Settings → Polish. */
  builtinChosen: boolean;
  onBuiltinChange: (on: boolean) => void;
  saving: boolean;
  /** Settings → Polish opens once setup is done. */
  ownProviderLater: boolean;
  onOwnProvider: () => void;
  onNext: () => void;
}) {
  const ownReady = status?.own_provider_ready === true;
  // While the server is still being asked, show the saved choice: it is what the First try reads.
  const builtin = builtinPolishView(!status || status.builtin === "first_try" ? "ready" : status.builtin, builtinChosen);
  const on = ownReady ? status?.own_provider_enabled === true : builtin.checked;
  const note = ownReady
    ? on
      ? "Polish is on and set up with your own AI provider."
      : `Polish is set up with your own AI provider but off. Turn it on in Settings → Polish, or from Voicetypr’s ${isMacOS ? "menu bar" : "tray"} menu.`
    : on && !builtin.active
      ? builtin.status
      : on
        ? "Your words go to Voicetypr’s server to be cleaned, then straight back. Nothing is kept."
        : "No AI cleanup. Your Dictionary and voice commands still apply. Turn it on any time in Settings → Polish.";
  const NoteIcon = on ? ShieldCheck : Type;

  return (
    <section className="flex w-[540px] flex-col items-center gap-[30px]">
      <FrameHeading
        title="Let AI tidy your words"
        line="Polish is AI clean-up. It runs on Voicetypr’s server, so you need no AI key or account of your own."
      />
      <div className="flex flex-col self-stretch overflow-hidden rounded-2xl border border-border bg-card">
        <div className="flex items-center gap-3.5 px-[22px] py-5">
          <span className="flex size-[38px] shrink-0 items-center justify-center rounded-[9px] bg-sage-bg text-sage">
            <Sparkles aria-hidden="true" className="size-[19px]" />
          </span>
          <label htmlFor="onboarding-builtin-polish" className="flex-1 text-[17px] leading-[1.2] font-semibold text-foreground">
            Clean up what I say
          </label>
          {ownReady ? null : (
            <Switch
              id="onboarding-builtin-polish"
              checked={builtin.checked}
              disabled={saving}
              onCheckedChange={onBuiltinChange}
              className="data-[size=default]:h-[26px] data-[size=default]:w-11 [&_[data-slot=switch-thumb]]:size-[22px] [&_[data-slot=switch-thumb][data-checked]]:translate-x-[18px]"
            />
          )}
        </div>
        <div className="flex items-start gap-3 bg-secondary py-4 pr-4 pl-[22px]">
          <NoteIcon aria-hidden="true" className="mt-0.5 size-[15px] shrink-0 text-muted-foreground" />
          <p role="status" className="text-[15px] leading-[1.4] text-muted-foreground">{note}</p>
        </div>
      </div>
      {ownReady ? null : (
        <QuietButton className="text-[15px] text-sage hover:text-sage" aria-pressed={ownProviderLater} onClick={onOwnProvider}>
          {ownProviderLater ? <Check aria-hidden="true" className="size-[15px]" /> : <Settings2 aria-hidden="true" className="size-[15px]" />}
          {ownProviderLater ? "Settings → Polish opens after setup" : "Use my own AI provider"}
        </QuietButton>
      )}
      <PrimaryButton disabled={saving} onClick={onNext}>Continue</PrimaryButton>
    </section>
  );
}
