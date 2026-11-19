import { ChoiceCard } from "@/components/settings/settings-ui";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { isMacOS } from "@/lib/platform";
import { Cloud, Laptop, Network } from "lucide-react";

/** The three places dictation can run, each saying what it needs up front. */
export function SourceStep({
  sourceType,
  onConfirmSource,
}: {
  sourceType: SourceType;
  onConfirmSource: (source: SourceType) => void;
}) {
  return (
    <div className="flex w-full flex-col gap-[10px]">
      <ChoiceCard
        layout="row"
        label={isMacOS ? "On this Mac" : "On this PC"}
        icon={Laptop}
        description="Private, works offline. You choose the model next."
        selected={sourceType === "local"}
        onSelect={() => onConfirmSource("local")}
      />
      <ChoiceCard
        layout="row"
        label="Cloud, with your key"
        icon={Cloud}
        description="Your own key from a provider such as Soniox or Deepgram. Your audio goes to that provider."
        selected={sourceType === "cloud"}
        onSelect={() => onConfirmSource("cloud")}
      />
      <ChoiceCard
        layout="row"
        label="Another computer on your network"
        icon={Network}
        description="A Mac or PC running Voicetypr with sharing on, on a trusted network."
        selected={sourceType === "remote"}
        onSelect={() => onConfirmSource("remote")}
      />
    </div>
  );
}
