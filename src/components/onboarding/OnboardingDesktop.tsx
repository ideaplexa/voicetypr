import { HotkeyStep } from "@/components/onboarding/HotkeyStep";
import { StepDots } from "@/components/onboarding/OnboardingChrome";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";
import { ReadinessStep } from "@/components/onboarding/ReadinessStep";
import { SourceStep } from "@/components/onboarding/SourceStep";
import { SuccessStep } from "@/components/onboarding/SuccessStep";
import { WelcomeStep } from "@/components/onboarding/WelcomeStep";
import { sourceLabel, type OnboardingDesktopProps } from "@/components/onboarding/onboardingTypes";
import { useOnboardingDesktop } from "@/components/onboarding/useOnboardingDesktop";

export const OnboardingDesktop = function OnboardingDesktop(props: OnboardingDesktopProps) {
  const {
    currentStep,
    currentIndex,
    steps,
    sourceType,
    confirmSource,
    handleBack,
    handleNext,
    nextDisabled,
    permissions,
    checkingPermissions,
    isRequestingPermission,
    checkSinglePermission,
    requestPermission,
    local,
    cloud,
    remote,
    hotkey,
    holdToTalk,
    capturedBareModifier,
    onHotkeyChange,
    onEditingChange,
    onBareModifier,
    onHoldToTalkChange,
    telemetryOptIn,
    analyticsOptIn,
    isSavingCompletion,
    onTelemetryChange,
    onAnalyticsChange,
    completeOnboarding,
  } = useOnboardingDesktop(props);

  return (
    <div className="min-h-screen overflow-hidden bg-[radial-gradient(110%_80%_at_50%_-10%,var(--sage-bg),transparent_55%),linear-gradient(180deg,var(--background),var(--background))] text-foreground">
      {currentStep !== "success" && (
        <div className="mx-auto flex w-full max-w-5xl items-center justify-between gap-4 px-8 py-6">
          <div>
            <p className="text-sm font-semibold tracking-tight">Voicetypr Setup</p>
            <p className="text-xs text-muted-foreground">{sourceLabel(sourceType)}</p>
          </div>
          <StepDots currentIndex={currentIndex} total={steps.length} />
        </div>
      )}

      <main className="mx-auto flex min-h-[calc(100vh-76px)] w-full max-w-5xl items-center justify-center px-8 pb-10">
        {currentStep === "welcome" && <WelcomeStep onNext={handleNext} />}

        {currentStep === "source" && (
          <SourceStep
            sourceType={sourceType}
            onConfirmSource={confirmSource}
            onBack={handleBack}
            onNext={handleNext}
            nextDisabled={nextDisabled}
          />
        )}

        {currentStep === "permissions" && (
          <PermissionsStep
            permissions={permissions}
            checkingPermissions={checkingPermissions}
            isRequestingPermission={isRequestingPermission}
            onCheck={checkSinglePermission}
            onRequest={requestPermission}
            onBack={handleBack}
            onNext={handleNext}
            nextDisabled={nextDisabled}
          />
        )}

        {currentStep === "readiness" && (
          <ReadinessStep
            sourceType={sourceType}
            onBack={handleBack}
            onNext={handleNext}
            nextDisabled={nextDisabled}
            local={local}
            cloud={cloud}
            remote={remote}
          />
        )}

        {currentStep === "hotkey" && (
          <HotkeyStep
            hotkey={hotkey}
            holdToTalk={holdToTalk}
            capturedBareModifier={capturedBareModifier}
            onHotkeyChange={onHotkeyChange}
            onEditingChange={onEditingChange}
            onBareModifier={onBareModifier}
            onHoldToTalkChange={onHoldToTalkChange}
            onBack={handleBack}
            onNext={handleNext}
            nextDisabled={nextDisabled}
          />
        )}

        {currentStep === "success" && (
          <SuccessStep
            capturedBareModifier={capturedBareModifier}
            holdToTalk={holdToTalk}
            hotkey={hotkey}
            telemetryOptIn={telemetryOptIn}
            analyticsOptIn={analyticsOptIn}
            isSavingCompletion={isSavingCompletion}
            onTelemetryChange={onTelemetryChange}
            onAnalyticsChange={onAnalyticsChange}
            onComplete={completeOnboarding}
          />
        )}
      </main>
    </div>
  );
};
