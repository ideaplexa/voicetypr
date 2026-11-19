import { Button } from "@/components/settings/SettingsButton";
import { FounderLine } from "@/components/onboarding/FounderLine";
import { isMacOS } from "@/lib/platform";
import { Check } from "lucide-react";

const machine = isMacOS ? "Mac" : "PC";

/** What a license gives, from what the license gate covers in the app. */
export const BUY_FEATURES = [
  "Dictation in every app on this " + machine,
  "Every on-device model, and your own cloud keys",
  "Network sharing with your other computers",
  "The command line: transcribe files and record from the terminal",
  "Direct help from Moinul, who reads every email",
];

/** The last screen before Done, after the first try: buy, or start the trial. */
export function BuyStep({
  trialEnded,
  pending,
  onBuy,
  onStartTrial,
  onHaveKey,
  onBack,
  onDecideLater,
}: {
  /** This device already used its trial, so only buying or a key can unlock it. */
  trialEnded: boolean;
  pending: boolean;
  onBuy: () => void;
  /** Goes to the name and email page. */
  onStartTrial: () => void;
  onHaveKey: () => void;
  onBack: () => void;
  onDecideLater: () => void;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-5 text-center">
      <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">Keep Voicetypr</h2>
      {trialEnded ? (
        <p className="max-w-[480px] text-sm text-muted-foreground">
          This {machine}’s free week has been used. Buy Voicetypr, or activate a key you already have, to keep dictating.
        </p>
      ) : null}
      <ul className="flex flex-col gap-2.5 text-left">
        {BUY_FEATURES.map((feature) => (
          <li key={feature} className="flex items-start gap-2 text-sm">
            <Check className="mt-0.5 size-4 shrink-0 text-sage" />
            {feature}
          </li>
        ))}
      </ul>
      <div className="flex w-full max-w-[320px] flex-col gap-[10px]">
        <Button size="lg" disabled={pending} onClick={onBuy}>Buy now</Button>
        {trialEnded ? null : (
          <Button size="lg" variant="outline" disabled={pending} onClick={onStartTrial}>Start 7-day trial</Button>
        )}
      </div>
      <p className="text-sm text-muted-foreground">
        Already bought it?{" "}
        <button type="button" disabled={pending} className="font-medium text-foreground underline underline-offset-2 disabled:opacity-50" onClick={onHaveKey}>
          I have a key
        </button>
      </p>
      <FounderLine />
      <div className="flex gap-[10px]">
        <Button variant="ghost" size="sm" className="text-muted-foreground" disabled={pending} onClick={onBack}>Back</Button>
        <Button variant="ghost" size="sm" className="text-muted-foreground" disabled={pending} onClick={onDecideLater}>Decide later</Button>
      </div>
    </section>
  );
}
