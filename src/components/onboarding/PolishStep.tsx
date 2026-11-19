import { Button } from "@/components/settings/SettingsButton";
import { Switch } from "@/components/settings/SettingsSwitch";
import { builtinPolishView } from "@/components/polish/BuiltInPolishSetting";
import { Card } from "@/components/ui/card";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import type { PolishStatus } from "@/types";
import { Sparkles } from "lucide-react";

/** Whether Polish cleans what the user says: one switch, shared with Settings → Polish. */
export function PolishStep({
  status,
  builtinChosen,
  onBuiltinChange,
  saving,
  onBack,
  onNext,
}: {
  /** `polish_status`, the gate dictation uses; null while it is being asked. */
  status: PolishStatus | null;
  /** The saved built-in Polish choice, shared with Settings → Polish. */
  builtinChosen: boolean;
  onBuiltinChange: (on: boolean) => void;
  saving: boolean;
  onBack: () => void;
  onNext: () => void;
}) {
  const ownReady = status?.own_provider_ready === true;
  const builtin = builtinPolishView(ownReady ? "own_provider" : status?.builtin === "first_try" ? "ready" : status?.builtin ?? null, builtinChosen);
  const on = ownReady ? status?.own_provider_enabled === true : builtin.checked;

  return (
    <section className="flex w-full flex-col items-center gap-[16px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        Polish tidies what you say
      </h2>
      <p className="max-w-[540px] text-center text-base text-muted-foreground">
        It drops filler and false starts and fixes punctuation, on Voicetypr’s server, so you need no AI key of your own.
      </p>
      <Card className="w-full gap-3 px-[18px] py-4 text-left">
        {ownReady ? (
          <div className="flex items-start gap-3">
            <Sparkles className={cn("mt-0.5 size-4 shrink-0", on ? "text-sage" : "text-muted-foreground")} />
            <p className="text-sm">
              {on ? (
                <><span className="font-semibold">Polish is on</span> and set up with your AI provider.</>
              ) : (
                <><span className="font-semibold">Polish is set up but off.</span> Turn it on in Settings → Polish, or from Voicetypr’s {isMacOS ? "menu bar" : "tray"} menu.</>
              )}
            </p>
          </div>
        ) : (
          <div className="flex items-start gap-3">
            <Sparkles className={cn("mt-0.5 size-4 shrink-0", on ? "text-sage" : "text-muted-foreground")} />
            <div className="flex-1 text-sm">
              <label htmlFor="onboarding-builtin-polish" className="font-semibold">Clean up what I say</label>
              <p className="text-muted-foreground">
                Your words go to Voicetypr’s server to be cleaned. Voicetypr’s server keeps nothing. Off sends nothing; you get
                exactly what you said.
              </p>
              {builtin.checked ? null : <p className="mt-1">{builtin.status}</p>}
            </div>
            <Switch id="onboarding-builtin-polish" checked={builtin.checked} disabled={builtin.disabled || saving}
              onCheckedChange={onBuiltinChange} />
          </div>
        )}
        {ownReady ? null : (
          <p className="pl-7 text-sm text-muted-foreground">Rather use your own AI provider? Add its key in Settings → Polish.</p>
        )}
      </Card>
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" disabled={saving} onClick={onBack}>Back</Button>
        <Button disabled={saving} onClick={onNext}>Continue</Button>
      </div>
    </section>
  );
}
