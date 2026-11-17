import type { Step } from "@/components/onboarding/onboardingTypes";

let step: Step | undefined;
export function installOnboardingPreview(value: Step) {
  step = value;
}
export function getOnboardingPreview() {
  return step;
}
