import { useEffect, useRef, type RefObject } from "react";
import { updateService } from "@/services/updateService";
import { createLogger } from "@/lib/logger";

const log = createLogger("app");

interface UseOnboardingRecoveryOptions {
  forceShowOnboarding: boolean;
  setForceShowOnboarding: (value: boolean) => void;
  hasModels: boolean | null | undefined;
  forceOnboardingNeedsFreshAvailabilityRef: RefObject<boolean>;
  showOnboarding: boolean;
  hasCompletedOnboardingRef: RefObject<boolean>;
  checkAccessibilityPermission: () => Promise<unknown>;
  checkMicrophonePermission: () => Promise<unknown>;
}

export function useOnboardingRecovery({
  forceShowOnboarding,
  setForceShowOnboarding,
  hasModels,
  forceOnboardingNeedsFreshAvailabilityRef,
  showOnboarding,
  hasCompletedOnboardingRef,
  checkAccessibilityPermission,
  checkMicrophonePermission,
}: UseOnboardingRecoveryOptions) {
  const previousHasModelsRef = useRef<boolean | null>(hasModels ?? null);

  useEffect(() => {
    const previous = previousHasModelsRef.current;
    previousHasModelsRef.current = hasModels ?? null;

    if (!(forceShowOnboarding && hasModels === true)) {
      return;
    }

    if (forceOnboardingNeedsFreshAvailabilityRef.current && previous === true) {
      forceOnboardingNeedsFreshAvailabilityRef.current = false;
      return;
    }

    forceOnboardingNeedsFreshAvailabilityRef.current = false;
    const timeoutId = window.setTimeout(() => {
      setForceShowOnboarding(false);
    }, 0);

    return () => window.clearTimeout(timeoutId);
  }, [
    forceShowOnboarding,
    hasModels,
    forceOnboardingNeedsFreshAvailabilityRef,
    setForceShowOnboarding,
  ]);

  // Check permissions only after an explicit onboarding completion.
  useEffect(() => {
    if (!showOnboarding && hasCompletedOnboardingRef.current) {
      hasCompletedOnboardingRef.current = false;

      Promise.all([checkAccessibilityPermission(), checkMicrophonePermission()]).then(() => {
        log.info("Permissions refreshed after onboarding completion");
      });

      updateService.requestNotificationPermission();
    }
  }, [
    showOnboarding,
    hasCompletedOnboardingRef,
    checkAccessibilityPermission,
    checkMicrophonePermission,
  ]);
}
