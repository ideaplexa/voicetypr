import { ChoiceCard } from "@/components/settings/settings-ui";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { isMacOS } from "@/lib/platform";
import { Cloud, Laptop, Network } from "lucide-react";

export function SourceStep({
  sourceType,
  onConfirmSource,
  modelSize,
}: {
  sourceType: SourceType;
  onConfirmSource: (source: SourceType) => void;
  modelSize: string | null;
}) {
  return (
    <div className="flex w-full flex-col gap-[10px]">
      <ChoiceCard
        layout="row"
        label={isMacOS ? "On this Mac" : "On this PC"}
        icon={Laptop}
        description={`Private, offline, free.${modelSize ? ` Downloads a ${modelSize} model once.` : " Choose a local model."}`}
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
