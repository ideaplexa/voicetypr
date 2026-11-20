import type { ComponentProps, ReactNode } from "react";
import { FrameHeading, PrimaryButton, machine } from "@/components/onboarding/Frame";
import { ReadinessCloudPanel } from "@/components/onboarding/ReadinessCloudPanel";
import { ReadinessLocalPanel } from "@/components/onboarding/ReadinessLocalPanel";
import { ReadinessRemotePanel } from "@/components/onboarding/ReadinessRemotePanel";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { cn } from "@/lib/utils";

const COPY: Record<SourceType, [string, string]> = {
  local: [`Choose a model for this ${machine}`, `It runs here, so your audio never leaves this ${machine}.`],
  cloud: [
    "Choose a cloud provider",
    `Your audio goes to the provider you pick. Your key is stored securely on this ${machine} and sent only to them.`,
  ],
  remote: ["Use another computer on your network", "Use only on a trusted network: audio and transcripts travel unencrypted."],
};

/** 04, 04b and 04c: the engine for the place chosen on 02. */
export function EngineStep({
  sourceType,
  local,
  cloud,
  remote,
  warning,
  ready,
  busy,
  onNext,
}: {
  sourceType: SourceType;
  local: ComponentProps<typeof ReadinessLocalPanel>;
  cloud: ComponentProps<typeof ReadinessCloudPanel>;
  remote: ComponentProps<typeof ReadinessRemotePanel>;
  /** A model that doesn't support the chosen language asks before downloading. */
  warning: ReactNode;
  /** Whether an engine is chosen; a local model may still be downloading. */
  ready: boolean;
  busy: boolean;
  onNext: () => void;
}) {
  const [title, line] = COPY[sourceType];
  const downloading = sourceType === "local" && !local.localReady && !!local.preparing && local.downloadProgress[local.preparing] !== undefined;
  return (
    <section className={cn("flex flex-col items-center", sourceType === "local" ? "w-[640px] gap-[26px]" : "w-[600px] gap-[30px]")}>
      <FrameHeading title={title} line={line} />
      {sourceType === "local" && <ReadinessLocalPanel {...local} />}
      {sourceType === "cloud" && <ReadinessCloudPanel {...cloud} />}
      {sourceType === "remote" && <ReadinessRemotePanel {...remote} />}
      {warning}
      <PrimaryButton disabled={busy || !ready} onClick={onNext}>
        {downloading ? "Continue while it downloads" : "Continue"}
      </PrimaryButton>
    </section>
  );
}
