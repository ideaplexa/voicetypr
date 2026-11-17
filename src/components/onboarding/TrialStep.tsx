import { TrialStartForm } from "@/components/TrialStartForm";
import { Button } from "@/components/settings/SettingsButton";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { isMacOS } from "@/lib/platform";
import type { TrialStartError } from "@/types";
import { FounderLine } from "@/components/onboarding/FounderLine";
import { CreditCard, Laptop, Lock } from "lucide-react";
import type { ReactNode } from "react";

export function TrialStep({
  trialEnded,
  localDictation,
  onStartTrial,
  onUpgrade,
  onHaveKey,
  onBack,
  onDecideLater,
  consent,
}: {
  /** This device already used its trial, so only buying or a key can unlock it. */
  trialEnded: boolean;
  localDictation: boolean;
  onStartTrial: (name: string, email: string) => Promise<TrialStartError | null>;
  onUpgrade: () => void;
  onHaveKey: () => void;
  onBack: () => void;
  onDecideLater: () => void;
  consent: ReactNode;
}) {
  const machine = isMacOS ? "Mac" : "PC";
  return (
    <section className="flex w-full flex-col items-center gap-[18px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        {trialEnded ? "Trial ended" : "Start with 7 days free"}
      </h2>
      <div className="grid w-full gap-3 sm:grid-cols-[1.45fr_1fr]">
        {trialEnded ? (
          <Card className="gap-2 px-5 py-5">
            <h3 className="text-base font-semibold">Dictation is paused</h3>
            <p className="text-[13px] leading-5 text-muted-foreground">
              This {machine}’s free week has been used. Upgrade, or activate a key you already have, to keep dictating.
              Your history and settings stay on this {machine}.
            </p>
          </Card>
        ) : (
          <Card className="gap-3 px-5 py-5 outline-[1.5px] outline-sage -outline-offset-1">
            <h3 className="flex items-center gap-2 text-base font-semibold">
              Start 7-day trial <Badge variant="secondary">Recommended</Badge>
            </h3>
            <p className="text-[13px] leading-5 text-muted-foreground">
              Try Voicetypr for 7 days, free. No card. The clock starts when you click, not before.
            </p>
            <TrialStartForm onStart={onStartTrial} />
          </Card>
        )}
        <Card className="gap-3 px-5 py-5">
          <h3 className="text-base font-semibold">Upgrade now</h3>
          <p className="text-[13px] leading-5 text-muted-foreground">One payment, yours to keep.</p>
          <span className="flex-1" />
          <Button variant={trialEnded ? "default" : "outline"} onClick={onUpgrade}>Upgrade now</Button>
          <Button variant="ghost" onClick={onHaveKey}>I have a key</Button>
        </Card>
      </div>
      <ul className="flex flex-wrap justify-center gap-x-5 gap-y-1.5 text-[12.5px] text-muted-foreground">
        {localDictation ? (
          <li className="flex items-center gap-1.5"><Laptop className="size-3.5" />Dictation runs on this {machine}</li>
        ) : null}
        <li className="flex items-center gap-1.5"><CreditCard className="size-3.5" />No card</li>
        <li className="flex items-center gap-1.5"><Lock className="size-3.5" />Usage reports never include your words or audio</li>
      </ul>
      {consent}
      <div className="flex gap-[10px]">
        <Button variant="outline" size="sm" onClick={onBack}>Back</Button>
        <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={onDecideLater}>
          Decide later
        </Button>
      </div>
      <FounderLine />
    </section>
  );
}
