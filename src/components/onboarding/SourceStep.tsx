import { ChoiceCard } from "@/components/settings/settings-ui";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { isMacOS } from "@/lib/platform";
import { Cloud, Laptop, Network } from "lucide-react";

/** The three places dictation can run, each saying what it needs up front. */
export function SourceStep({
  sourceType,
  onConfirmSource,
  modelName,
  modelSize,
}: {
  sourceType: SourceType;
  onConfirmSource: (source: SourceType) => void;
  modelName: string | null;
  modelSize: string | null;
}) {
  return (
    <div className="flex w-full flex-col gap-[10px]">
      <ChoiceCard
        layout="row"
        label={isMacOS ? "On this Mac" : "On this PC"}
        icon={Laptop}
        description={modelName
          ? `${modelName}${modelSize ? ` · ${modelSize}, downloaded once` : ""}. Private, works offline.`
          : "Private, works offline. Needs a local model."}
        selected={sourceType === "local"}
        onSelect={() => onConfirmSource("local")}
      />
      <ChoiceCard
        layout="row"
        label="Cloud, with your key"
        icon={Cloud}
        description="Soniox, Deepgram, OpenAI, Groq or Cohere. Needs a valid API key from that provider; your audio goes to that provider."
        selected={sourceType === "cloud"}
        onSelect={() => onConfirmSource("cloud")}
      />
      <ChoiceCard
        layout="row"
        label="Another computer on your network"
        icon={Network}
        description="Needs a Mac or PC running Voicetypr with sharing on, on the same network. Use only on a trusted network: audio and transcripts travel unencrypted."
        selected={sourceType === "remote"}
        onSelect={() => onConfirmSource("remote")}
      />
    </div>
  );
}
