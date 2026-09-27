import { OnboardingPanel, StepFooter } from "@/components/onboarding/OnboardingChrome";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { Cloud, Laptop, Network } from "lucide-react";

export function SourceStep({
  sourceType,
  onConfirmSource,
  onBack,
  onNext,
  nextDisabled,
}: {
  sourceType: SourceType;
  onConfirmSource: (nextSourceType: SourceType) => void;
  onBack: () => void;
  onNext: () => void | Promise<void>;
  nextDisabled: boolean;
}) {
  return (
    <OnboardingPanel
      title="Choose where transcription runs"
      description="You can change this later from Models. Onboarding prepares the source you choose now."
      footer={
        <StepFooter
          onBack={onBack}
          onNext={onNext}
          nextDisabled={nextDisabled}
          nextLabel="Continue"
        />
      }
    >
      <ToggleGroup
        variant="outline"
        spacing={3}
        value={[sourceType]}
        onValueChange={(group) => {
          const value = group[0];
          if (value === "local" || value === "cloud" || value === "remote") {
            onConfirmSource(value);
          }
        }}
        aria-label="Transcription source"
        className="mx-auto grid h-auto w-full max-w-3xl grid-cols-1 bg-transparent md:grid-cols-3"
      >
        <ToggleGroupItem
          value="local"
          aria-label="Use a local model"
          className="h-full min-h-44 flex-col items-start justify-start gap-3 whitespace-normal rounded-2xl p-5 text-left data-pressed:border-sage/60 data-pressed:bg-sage-bg/50 data-pressed:text-foreground"
        >
          <span className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
            <Laptop className="size-5" />
          </span>
          <span className="text-base font-semibold">Local</span>
          <span className="text-sm leading-6 text-muted-foreground">
            Download a model and transcribe on this device. Works offline.
          </span>
        </ToggleGroupItem>
        <ToggleGroupItem
          value="cloud"
          aria-label="Use a cloud provider"
          className="h-full min-h-44 flex-col items-start justify-start gap-3 whitespace-normal rounded-2xl p-5 text-left data-pressed:border-sage/60 data-pressed:bg-sage-bg/50 data-pressed:text-foreground"
        >
          <span className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
            <Cloud className="size-5" />
          </span>
          <span className="text-base font-semibold">Cloud</span>
          <span className="text-sm leading-6 text-muted-foreground">
            Connect a provider with an API key. Audio is sent to that provider.
          </span>
        </ToggleGroupItem>
        <ToggleGroupItem
          value="remote"
          aria-label="Use another Voicetypr"
          className="h-full min-h-44 flex-col items-start justify-start gap-3 whitespace-normal rounded-2xl p-5 text-left data-pressed:border-sage/60 data-pressed:bg-sage-bg/50 data-pressed:text-foreground"
        >
          <span className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
            <Network className="size-5" />
          </span>
          <span className="text-base font-semibold">Remote Voicetypr</span>
          <span className="text-sm leading-6 text-muted-foreground">
            Use a model running in Voicetypr on another device on your network.
          </span>
        </ToggleGroupItem>
      </ToggleGroup>
    </OnboardingPanel>
  );
}
