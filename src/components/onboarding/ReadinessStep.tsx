import { OnboardingPanel, StepFooter } from "@/components/onboarding/OnboardingChrome";
import {
  ReadinessCloudPanel,
  type ReadinessCloudPanelProps,
} from "@/components/onboarding/ReadinessCloudPanel";
import {
  ReadinessLocalPanel,
  type ReadinessLocalPanelProps,
} from "@/components/onboarding/ReadinessLocalPanel";
import {
  ReadinessRemotePanel,
  type ReadinessRemotePanelProps,
} from "@/components/onboarding/ReadinessRemotePanel";
import { READINESS_COPY, type SourceType } from "@/components/onboarding/onboardingTypes";

export interface ReadinessStepProps {
  sourceType: SourceType;
  onBack: () => void;
  onNext: () => void | Promise<void>;
  nextDisabled: boolean;
  local: ReadinessLocalPanelProps;
  cloud: ReadinessCloudPanelProps;
  remote: ReadinessRemotePanelProps;
}

export function ReadinessStep({
  sourceType,
  onBack,
  onNext,
  nextDisabled,
  local,
  cloud,
  remote,
}: ReadinessStepProps) {
  return (
    <OnboardingPanel
      title={READINESS_COPY[sourceType].title}
      description={READINESS_COPY[sourceType].description}
      footer={
        <StepFooter
          onBack={onBack}
          onNext={onNext}
          nextDisabled={nextDisabled}
          nextLabel="Continue"
        />
      }
    >
      {sourceType === "local" ? <ReadinessLocalPanel {...local} /> : null}

      {sourceType === "cloud" ? <ReadinessCloudPanel {...cloud} /> : null}

      {sourceType === "remote" ? <ReadinessRemotePanel {...remote} /> : null}
    </OnboardingPanel>
  );
}
