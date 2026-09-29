let phase: 1 | 2 | 3 | undefined;
export function installOnboardingPreview(value: 1 | 2 | 3) {
  phase = value;
}
export function getOnboardingPreview() {
  return phase;
}
