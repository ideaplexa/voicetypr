import { TrialStartForm } from "@/components/TrialStartForm";
import { Button } from "@/components/settings/SettingsButton";
import { Card } from "@/components/ui/card";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import type { TrialStartError } from "@/types";
import { FounderLine } from "@/components/onboarding/FounderLine";
import { TermsLine } from "@/components/onboarding/ConsentChoice";
import { Check, CreditCard, Laptop, Lock } from "lucide-react";
import type { ReactNode } from "react";

function Point({ children }: { children: ReactNode }) {
  return (
    <li className="flex items-start gap-2 text-sm text-muted-foreground">
      <Check className="mt-0.5 size-4 shrink-0 text-sage" />
      {children}
    </li>
  );
}

export function TrialStep({
  trialEnded,
  localDictation,
  pending,
  onStartTrial,
  onUpgrade,
  onHaveKey,
  onBack,
  onDecideLater,
}: {
  /** This device already used its trial, so only buying or a key can unlock it; only then is Upgrade here. */
  trialEnded: boolean;
  localDictation: boolean;
  /** A trial start is in flight: leaving now would let it be sent twice. */
  pending: boolean;
  onStartTrial: (name: string, email: string) => Promise<TrialStartError | null>;
  onUpgrade: () => void;
  onHaveKey: () => void;
  onBack: () => void;
  onDecideLater: () => void;
}) {
  const machine = isMacOS ? "Mac" : "PC";
  const upgradeCard = (
    <Card className="gap-3 px-5 py-5 text-left">
      <h3 className="text-[17px] font-semibold">Upgrade now</h3>
      <ul className="flex flex-col gap-2">
        <Point>One payment, yours to keep.</Point>
        <Point>Choose a license on voicetypr.com, then paste the key from your receipt.</Point>
        <Point>Your history and settings carry on as they are.</Point>
      </ul>
      <span className="flex-1" />
      <Button type="button" size="lg" variant={trialEnded ? "default" : "outline"} className="w-full" disabled={pending} onClick={onUpgrade}>
        Upgrade now
      </Button>
    </Card>
  );
  const trialCard = (start: ReactNode, notice: ReactNode) => (
    <Card className="w-full max-w-[420px] gap-3 self-center px-5 py-5 text-left">
      <h3 className="text-[17px] font-semibold">7 days free</h3>
      <ul className="flex flex-col gap-2">
        <Point>No card.</Point>
        <Point>The clock starts when you click, not before.</Point>
      </ul>
      {start}
      {notice}
    </Card>
  );
  return (
    <section className="flex w-full flex-col items-center gap-5">
      <h2 className="text-center text-[28px] leading-[normal] font-semibold tracking-[-0.5px]">
        {trialEnded ? "Trial ended" : "Start with 7 days free"}
      </h2>
      {trialEnded ? (
        <div className="grid w-full gap-3 sm:grid-cols-2">
          <Card className="gap-3 px-5 py-5 text-left">
            <h3 className="text-[17px] font-semibold">Dictation is paused</h3>
            <p className="text-sm leading-6 text-muted-foreground">
              This {machine}’s free week has been used. Upgrade, or activate a key you already have, to keep
              dictating. Your history and settings stay on this {machine}.
            </p>
          </Card>
          {upgradeCard}
        </div>
      ) : (
        <TrialStartForm
          large
          onStart={onStartTrial}
          className="gap-4"
          renderAction={(button, notice) => trialCard(button, <div className="[&>p]:text-left [&>p]:text-xs">{notice}</div>)}
        />
      )}
      <p className="text-sm text-muted-foreground">
        Already bought it?{" "}
        <button type="button" disabled={pending} className="font-medium text-foreground underline underline-offset-2 disabled:opacity-50" onClick={onHaveKey}>
          I have a key
        </button>
      </p>
      <ul className={cn("flex flex-wrap items-center justify-center gap-x-5 gap-y-2 text-[13px] text-muted-foreground")}>
        {localDictation ? (
          <li className="flex items-center gap-1.5"><Laptop className="size-4" />Dictation runs on this {machine}</li>
        ) : null}
        {trialEnded ? null : <li className="flex items-center gap-1.5"><CreditCard className="size-4" />No card for the trial</li>}
        <li className="flex items-center gap-1.5"><Lock className="size-4" />Usage reports never include your words or audio</li>
        <li className="flex items-center gap-1.5"><FounderLine /></li>
      </ul>
      <p className="flex flex-wrap items-center justify-center gap-1 text-xs text-muted-foreground">
        <TermsLine action={trialEnded ? "continuing" : "starting"} />
      </p>
      <div className="flex gap-[10px]">
        <Button variant="ghost" size="sm" className="text-muted-foreground" disabled={pending} onClick={onBack}>Back</Button>
        <Button variant="ghost" size="sm" className="text-muted-foreground" disabled={pending} onClick={onDecideLater}>Decide later</Button>
      </div>
    </section>
  );
}
