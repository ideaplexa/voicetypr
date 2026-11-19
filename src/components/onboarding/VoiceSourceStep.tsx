import { LanguageSelection } from "@/components/LanguageSelection";
import { Button } from "@/components/settings/SettingsButton";
import { SourceStep } from "@/components/onboarding/SourceStep";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { isMacOS } from "@/lib/platform";

/** "Where should your voice become text?": only the choice; the engine comes next. */
export function VoiceSourceStep({
  sourceType,
  language,
  busy,
  onConfirmSource,
  onLanguageChange,
  onBack,
  onNext,
}: {
  sourceType: SourceType;
  language: string;
  busy: boolean;
  onConfirmSource: (source: SourceType) => void;
  onLanguageChange: (value: string) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-[18px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        Where should your voice become text?
      </h2>
      <p className="text-center text-sm text-muted-foreground">
        Most people start on this {isMacOS ? "Mac" : "PC"}. You can change this any time in Settings.
      </p>
      <div className="flex items-center gap-3 self-start text-sm">
        <span>I speak</span>
        <LanguageSelection value={language} onValueChange={onLanguageChange} />
      </div>
      <SourceStep sourceType={sourceType} onConfirmSource={onConfirmSource} />
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" disabled={busy} onClick={onBack}>Back</Button>
        <Button disabled={busy} onClick={onNext}>Continue</Button>
      </div>
    </section>
  );
}
