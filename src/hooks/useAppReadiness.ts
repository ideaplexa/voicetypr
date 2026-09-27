import { useAccessibilityPermission } from "./useAccessibilityPermission";
import { useMicrophonePermission } from "./useMicrophonePermission";
import { useModelAvailabilityContext } from "@/contexts/ModelAvailabilityContext";
import { useLicenseStatus } from "./useLicenseStatus";
import { useSettings } from "@/contexts/SettingsContext";

/**
 * Computed hook that combines all domain-specific hooks to provide derived readiness values.
 * This hook does NOT manage state - it only computes values based on the domain hooks.
 */
export function useAppReadiness() {
  const { settings } = useSettings();
  const onboardingCompleted = settings?.onboarding_completed === true;

  const accessibility = useAccessibilityPermission({ checkOnMount: onboardingCompleted });
  const microphone = useMicrophonePermission({ checkOnMount: onboardingCompleted });
  const models = useModelAvailabilityContext();
  const license = useLicenseStatus();

  // Compute derived values
  const canRecord = Boolean(
    microphone.hasPermission &&
    models.hasModels &&
    models.selectedModelAvailable &&
    license.isValid &&
    !license.isChecking,
  );

  const canAutoInsert = Boolean(accessibility.hasPermission);

  const isFullyReady = Boolean(
    microphone.hasPermission &&
    accessibility.hasPermission &&
    models.hasModels &&
    models.selectedModelAvailable &&
    license.isValid &&
    !license.isChecking,
  );

  // Check if any hook is still loading
  const isLoading =
    accessibility.isChecking ||
    microphone.isChecking ||
    models.isChecking ||
    license.isChecking ||
    accessibility.hasPermission === null ||
    microphone.hasPermission === null ||
    models.hasModels === null ||
    license.licenseStatus === null;

  return {
    // Individual states (for debugging/specific UI needs)
    hasAccessibilityPermission: accessibility.hasPermission,
    hasMicrophonePermission: microphone.hasPermission,
    hasModels: models.hasModels,
    selectedModelAvailable: models.selectedModelAvailable,
    licenseValid: license.isValid,
    licenseStatus: license.licenseStatus?.status ?? null,
    remoteSelected: models.remoteSelected,
    remoteAvailable: models.remoteAvailable,
    remoteStatus: models.remoteStatus,

    // Computed values
    canRecord,
    canAutoInsert,
    isFullyReady,
    isLoading,

    // Actions from domain hooks
    requestAccessibilityPermission: accessibility.requestPermission,
    requestMicrophonePermission: microphone.requestPermission,
    checkAccessibilityPermission: accessibility.checkPermission,
    checkMicrophonePermission: microphone.checkPermission,
    checkModels: models.checkModels,
    checkLicense: license.checkLicense,
  };
}
