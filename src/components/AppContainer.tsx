import { useCallback, useRef, useState, type SetStateAction } from "react";
import { AppErrorBoundary } from "./ErrorBoundary";
import { AppShell } from "./AppShell";
import type { ScreenId } from "./navigation";
import { OnboardingDesktop } from "./onboarding/OnboardingDesktop";
import { UpdateAnnouncementDialog } from "./UpdateAnnouncementDialog";
import { PrivacyConsentDialog } from "./PrivacyConsentDialog";
import { useReadiness } from "@/contexts/ReadinessContext";
import { useSettings } from "@/contexts/SettingsContext";
import { useInAppRecordingHotkey } from "@/hooks/useInAppRecordingHotkey";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { useModelAvailabilityContext } from "@/contexts/ModelAvailabilityContext";
import { useAppBootstrap } from "./app/useAppBootstrap";
import { useAppEvents } from "./app/useAppEvents";
import { useOnboardingRecovery } from "./app/useOnboardingRecovery";

import type { SourceFilter } from "./sections/models/types";

export function AppContainer() {
  const [{ activeSection, sourceFilter }, setNavigation] = useState<{
    activeSection: ScreenId;
    sourceFilter?: SourceFilter;
  }>({ activeSection: "overview" });
  const setActiveSection = useCallback((action: SetStateAction<ScreenId>) => {
    setNavigation((current) => {
      const next = typeof action === "function" ? action(current.activeSection) : action;
      // Destinations apply to one Sources visit; returning derives the latest source.
      return {
        activeSection: next,
        sourceFilter: next === "models" ? current.sourceFilter : undefined,
      };
    });
  }, []);
  const setSourceFilter = useCallback((filter: SourceFilter) => {
    setNavigation((current) => ({ ...current, sourceFilter: filter }));
  }, []);
  const [forceShowOnboarding, setForceShowOnboarding] = useState(false);
  const { settings, refreshSettings } = useSettings();
  const { checkAccessibilityPermission, checkMicrophonePermission } = useReadiness();
  const modelAvailability = useModelAvailabilityContext();

  // Use the model management context for onboarding
  const modelManagement = useModelManagementContext();

  // In-app fallback: toggle recording when the global hotkey is swallowed by a
  // focused WebView2 text field (e.g. Ctrl+Space in the bug-report box).
  useInAppRecordingHotkey();

  // Track explicit onboarding completion so recovery-driven onboarding doesn't trigger post-onboarding effects.
  const hasCompletedOnboardingRef = useRef(false);
  const forceOnboardingNeedsFreshAvailabilityRef = useRef(false);

  const { justUpdatedVersion, setJustUpdatedVersion } = useAppBootstrap(settings);

  useAppEvents({
    checkModels: modelAvailability.checkModels,
    setActiveSection,
    setSourceFilter,
    setForceShowOnboarding,
    forceOnboardingNeedsFreshAvailabilityRef,
  });

  const showOnboarding = Boolean(
    settings?.onboarding_completed === false ||
    forceShowOnboarding ||
    modelAvailability.hasModels === false,
  );

  useOnboardingRecovery({
    forceShowOnboarding,
    setForceShowOnboarding,
    hasModels: modelAvailability.hasModels,
    forceOnboardingNeedsFreshAvailabilityRef,
    showOnboarding,
    hasCompletedOnboardingRef,
    checkAccessibilityPermission,
    checkMicrophonePermission,
  });

  const markOnboardingCompletionStarted = () => {
    hasCompletedOnboardingRef.current = true;
  };

  const clearOnboardingCompletionMarker = () => {
    hasCompletedOnboardingRef.current = false;
  };

  // Onboarding View
  if (showOnboarding) {
    return (
      <AppErrorBoundary>
        <OnboardingDesktop
          onCompletionStart={markOnboardingCompletionStarted}
          onCompletionError={clearOnboardingCompletionMarker}
          onComplete={() => {
            setForceShowOnboarding(false);
            refreshSettings();
            void modelAvailability.checkModels();
          }}
          modelManagement={modelManagement}
        />
      </AppErrorBoundary>
    );
  }

  // Main App Layout
  return (
    <>
      <AppShell
        activeSection={activeSection}
        onSectionChange={setActiveSection}
        sourceFilter={sourceFilter}
        onSourceFilterChange={setSourceFilter}
      />
      <PrivacyConsentDialog />
      <UpdateAnnouncementDialog
        version={justUpdatedVersion}
        onClose={() => setJustUpdatedVersion(null)}
      />
    </>
  );
}
