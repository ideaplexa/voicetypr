import { Button } from "@/components/settings/SettingsButton";
import { isMacOS } from "@/lib/platform";
import { Check } from "lucide-react";

const machine = isMacOS ? "Mac" : "PC";

/** What a license keeps after the trial, from what the license gate covers in the app. */
export const KEEP_FEATURES = [
  "Dictation in every app on this " + machine + ", with no end date after your 7 days",
  "Every on-device model, and your own cloud keys",
  "Network sharing with your other computers",
  "The command line: transcribe files and record from the terminal",
  "Your history, dictionary and settings carry on as they are",
  "Direct help from Moinul, who reads every email",
];

/** Near the end, after the first try: what upgrading keeps, with an equal way on. */
export function KeepStep({
  trialEnds,
  onUpgrade,
  onKeepGoing,
  onBack,
}: {
  /** When the trial ends, in words; null when the date is not known. */
  trialEnds: string | null;
  onUpgrade: () => void;
  onKeepGoing: () => void;
  onBack: () => void;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-5 text-center">
      <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">Keep Voicetypr</h2>
      {trialEnds ? <p className="text-sm text-muted-foreground">{trialEnds}</p> : null}
      <ul className="flex flex-col gap-2.5 text-left">
        {KEEP_FEATURES.map((feature) => (
          <li key={feature} className="flex items-start gap-2 text-sm">
            <Check className="mt-0.5 size-4 shrink-0 text-sage" />
            {feature}
          </li>
        ))}
      </ul>
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button onClick={onUpgrade}>Upgrade now</Button>
        <Button variant="outline" onClick={onKeepGoing}>Keep going with the trial</Button>
      </div>
      <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={onBack}>Back</Button>
    </section>
  );
}
