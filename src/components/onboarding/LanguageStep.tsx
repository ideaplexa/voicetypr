import { LanguageSelection } from "@/components/LanguageSelection";
import { Button } from "@/components/settings/SettingsButton";

/** The language the user speaks: it decides which models fit. */
export function LanguageStep({
  language,
  busy,
  onLanguageChange,
  onBack,
  onNext,
}: {
  language: string;
  busy: boolean;
  onLanguageChange: (value: string) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-[18px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">What language do you speak?</h2>
      <p className="text-center text-sm text-muted-foreground">The models on the next step are the ones that understand it.</p>
      <LanguageSelection value={language} onValueChange={onLanguageChange} />
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" disabled={busy} onClick={onBack}>Back</Button>
        <Button disabled={busy} onClick={onNext}>Continue</Button>
      </div>
    </section>
  );
}
