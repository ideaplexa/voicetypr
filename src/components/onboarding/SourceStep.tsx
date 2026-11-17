import { ChoiceCard } from "@/components/settings/settings-ui";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { isMacOS } from "@/lib/platform";
import { Cloud, Laptop, Network } from "lucide-react";
import { LanguageSelection } from "@/components/LanguageSelection";

export function SourceStep({
  sourceType,
  onConfirmSource,
  modelSize,
  language,
  onLanguageChange,
}: {
  sourceType: SourceType;
  onConfirmSource: (source: SourceType) => void;
  modelSize: string | null;
  language: string;
  onLanguageChange: (value: string) => void;
}) {
  return (
    <div className="flex w-full flex-col gap-[10px]">
      <div className="mb-2 flex items-center justify-center gap-3 text-sm">
        <span>I speak</span>
        <LanguageSelection value={language} onValueChange={onLanguageChange} />
      </div>
      <ChoiceCard
        layout="row"
        label={isMacOS ? "On this Mac" : "On this PC"}
        icon={Laptop}
        description={`Private and offline, with no usage fees.${modelSize ? ` Downloads a ${modelSize} model once.` : " Choose a local model."}`}
        tag="Recommended"
        selected={sourceType === "local"}
        onSelect={() => onConfirmSource("local")}
      />
      <ChoiceCard
        layout="row"
        label="Cloud"
        icon={Cloud}
        description="Paste an API key from Soniox, Deepgram, OpenAI…"
        tag="Bring your key"
        selected={sourceType === "cloud"}
        onSelect={() => onConfirmSource("cloud")}
      />
      <ChoiceCard
        layout="row"
        label="Another computer"
        icon={Network}
        description="Use Voicetypr running on a stronger machine nearby."
        tag="Local network"
        selected={sourceType === "remote"}
        onSelect={() => onConfirmSource("remote")}
      />
    </div>
  );
}
