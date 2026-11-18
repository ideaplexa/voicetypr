import type { ComponentProps, ReactNode } from "react";
import { Button } from "@/components/settings/SettingsButton";
import { ReadinessCloudPanel } from "@/components/onboarding/ReadinessCloudPanel";
import { ReadinessLocalPanel } from "@/components/onboarding/ReadinessLocalPanel";
import { ReadinessRemotePanel } from "@/components/onboarding/ReadinessRemotePanel";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { isMacOS } from "@/lib/platform";

const COPY: Record<SourceType, [string, string]> = {
  local: [
    "Choose your model",
    "Runs on this " + (isMacOS ? "Mac" : "PC") + ": downloaded once, private, works offline.",
  ],
  cloud: ["Choose your cloud provider", "Add your key from that provider. Your audio goes to that provider."],
  remote: [
    "Choose the computer",
    "A Mac or PC running Voicetypr with sharing on. Use only on a trusted network: audio and transcripts travel unencrypted.",
  ],
};

/** The engine for the place chosen on the step before: models, providers or computers. */
export function EngineStep({
  sourceType,
  local,
  cloud,
  remote,
  warning,
  busy,
  onBack,
  onNext,
}: {
  sourceType: SourceType;
  local: ComponentProps<typeof ReadinessLocalPanel>;
  cloud: ComponentProps<typeof ReadinessCloudPanel>;
  remote: ComponentProps<typeof ReadinessRemotePanel>;
  /** A model that doesn't support the chosen language asks before downloading. */
  warning: ReactNode;
  busy: boolean;
  onBack: () => void;
  onNext: () => void;
}) {
  const [title, lede] = COPY[sourceType];
  return (
    <section className="flex w-full flex-col items-center gap-[18px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">{title}</h2>
      <p className="text-center text-sm text-muted-foreground">{lede}</p>
      <div className="w-full">
        {sourceType === "local" && <ReadinessLocalPanel {...local} />}
        {sourceType === "cloud" && <ReadinessCloudPanel {...cloud} />}
        {sourceType === "remote" && <ReadinessRemotePanel {...remote} />}
      </div>
      {warning}
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" disabled={busy} onClick={onBack}>Back</Button>
        <Button disabled={busy} onClick={onNext}>Continue</Button>
      </div>
    </section>
  );
}
