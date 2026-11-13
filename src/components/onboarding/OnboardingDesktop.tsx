import { HotkeyStep } from "@/components/onboarding/HotkeyStep";
import { StepDots } from "@/components/onboarding/OnboardingChrome";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";

import { SourceStep } from "@/components/onboarding/SourceStep";
import { SuccessStep } from "@/components/onboarding/SuccessStep";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/settings/SettingsButton";
import { MicrophoneCheck } from "@/components/onboarding/MicrophoneCheck";
import { isMacOS } from "@/lib/platform";
import { ReadinessLocalPanel } from "@/components/onboarding/ReadinessLocalPanel";
import { ReadinessCloudPanel } from "@/components/onboarding/ReadinessCloudPanel";
import { ReadinessRemotePanel } from "@/components/onboarding/ReadinessRemotePanel";
import { type OnboardingDesktopProps } from "@/components/onboarding/onboardingTypes";
import { useOnboardingDesktop } from "@/components/onboarding/useOnboardingDesktop";
import { OfflinePreparationStatus } from "@/components/onboarding/OfflinePreparationStatus";

export const OnboardingDesktop = function OnboardingDesktop(props: OnboardingDesktopProps) {
  const {
    currentStep,

    sourceType,
    preparation,
    sourceReady,
    openModelChoices,
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
    isSavingCompletion,
    onTelemetryChange,
    completeOnboarding,
  } = useOnboardingDesktop(props);

  const [savingShortcut, setSavingShortcut] = useState(false);
  const [editingShortcut, setEditingShortcut] = useState(false);
  const [windowsMicDone, setWindowsMicDone] = useState(props.previewPhase === 3);
  // Welcome remains a machine step, but shares phase one's chrome.
  useEffect(() => {
    if (currentStep === "welcome") void handleNext();
  }, [currentStep, handleNext]);
  const phase =
    currentStep === "permissions" || (currentStep === "hotkey" && !isMacOS && !windowsMicDone)
      ? 2
      : currentStep === "hotkey" || currentStep === "success"
        ? 3
        : 1;
  const preparingShortcut = useRef(false);
  useEffect(() => {
    if (currentStep !== "hotkey") {
      preparingShortcut.current = false;
      return;
    }
    if (phase !== 3 || editingShortcut || preparingShortcut.current || !sourceReady) return;
    preparingShortcut.current = true;
    setSavingShortcut(true);
    void handleNext().finally(() => {
      setEditingShortcut(true);
      setSavingShortcut(false);
    });
  }, [currentStep, phase, editingShortcut, handleNext, sourceReady]);
  const modelName = preparation.recommended;
  const model = modelName ? local.models[modelName] : undefined;
  const modelSize = model?.size ? `${Math.round(model.size / (1024 * 1024))} MB` : null;
  const needsDownload = model && !model.downloaded;
  const continueLabel =
    sourceType === "local" && needsDownload && modelSize
      ? `Continue — download ${model.display_name} (${modelSize})`
      : sourceType === "cloud"
        ? "Continue — connect your provider"
        : sourceType === "remote"
          ? "Continue — connect another computer"
          : "Continue";
  const phaseOneNext = async () => {
    await handleNext();
  };
  const enterTry = async () => {
    preparingShortcut.current = true;
    setSavingShortcut(true);
    await handleNext();
    setSavingShortcut(false);
    setEditingShortcut(true);
    setWindowsMicDone(true); // Save the real shortcut before the user tries dictation.
  };
  const actionClass =
    "h-auto rounded-[10px] px-4 py-[9px] text-[13px] leading-[normal] font-medium text-muted-foreground hover:text-muted-foreground";
  return (
    <div className="flex min-h-screen flex-col items-center bg-background pt-[44px] text-foreground">
      <StepDots currentIndex={phase - 1} total={3} />
      <main className="flex w-full flex-1 items-center justify-center px-6 py-6">
        <div
          className={`flex w-full max-w-[620px] flex-col items-center ${phase === 2 ? "gap-5" : "gap-[22px]"}`}
        >
          {phase === 1 && (
            <>
              <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
                Where should your voice become text?
              </h2>
              <p className="text-center text-sm leading-[normal] text-muted-foreground">
                You can change this any time. Most people start on this {isMacOS ? "Mac" : "PC"}.
              </p>
              {currentStep === "source" || currentStep === "welcome" ? (
                <SourceStep
                  sourceType={sourceType}
                  onConfirmSource={confirmSource}
                  modelSize={modelSize}
                  language={preparation.language}
                  onLanguageChange={(value) => void preparation.changeLanguage(value)}
                />
              ) : (
                <div className="w-full">
                  {sourceType === "local" && <ReadinessLocalPanel {...local} />}
                  {sourceType === "cloud" && <ReadinessCloudPanel {...cloud} />}
                  {sourceType === "remote" && <ReadinessRemotePanel {...remote} />}
                </div>
              )}
              <div className="flex gap-[10px]">
                {currentStep === "readiness" && (
                  <Button variant="outline" className={actionClass} onClick={handleBack}>
                    Back
                  </Button>
                )}
                <Button
                  className="h-auto rounded-[10px] px-4 py-[9px] border-0 text-sm leading-4"
                  disabled={nextDisabled || currentStep === "welcome"}
                  onClick={() => void phaseOneNext()}
                >
                  {currentStep === "readiness" ? "Continue" : continueLabel}
                </Button>
              </div>
              {(currentStep === "source" || currentStep === "welcome") && sourceType === "local" && (
                <Button variant="ghost" onClick={openModelChoices}>Choose a different model</Button>
              )}
            </>
          )}
          {phase === 2 && (
            <>
              <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
                {isMacOS ? "Let Voicetypr hear you and type for you" : "Check your microphone"}
              </h2>
              <p className="text-center text-sm leading-[normal] text-muted-foreground">
                {isMacOS
                  ? "Two permissions, then you're ready to try it."
                  : "Speak to check your microphone."}
              </p>
              {isMacOS && (
                <PermissionsStep
                  permissions={permissions}
                  checkingPermissions={checkingPermissions}
                  isRequestingPermission={isRequestingPermission}
                  onCheck={checkSinglePermission}
                  onRequest={requestPermission}
                />
              )}
              {(!isMacOS || permissions.microphone.status === "granted") && <MicrophoneCheck />}
              <div className="flex gap-[10px]">
                <Button variant="outline" className={actionClass} onClick={handleBack}>
                  Back
                </Button>
                <Button
                  className="h-auto rounded-[10px] px-4 py-[9px] border-0 text-sm leading-4"
                  disabled={(isMacOS ? nextDisabled : !sourceReady) || savingShortcut}
                  onClick={() => void (isMacOS ? handleNext() : enterTry())}
                >
                  Continue
                </Button>
              </div>
            </>
          )}
          {sourceType === "local" && currentStep !== "welcome" && currentStep !== "source" && (
            <OfflinePreparationStatus
              ready={local.localReady}
              hideWhenReady={phase === 3}
              verifying={Boolean(preparation.name && local.verifyingModels.has(preparation.name))}
              progress={preparation.name ? local.downloadProgress[preparation.name] : undefined}
              failed={Boolean(preparation.name && local.downloadErrors[preparation.name])}
              onCancel={() => { if (preparation.name) local.onCancelDownload(preparation.name); }}
              onRetry={() => { if (preparation.name) local.onDownload(preparation.name); }}
              onChoose={openModelChoices}
            />
          )}
          {preparation.warningModel && (
            <div role="alert" className="flex flex-col items-center gap-3 text-sm">
              <p>This model does not support your chosen language. Download it without selecting it?</p>
              <div className="flex gap-2">
                <Button variant="outline" onClick={preparation.dismissWarning}>Cancel</Button>
                <Button onClick={preparation.confirmDownload}>Download anyway</Button>
              </div>
            </div>
          )}
          {phase === 3 && currentStep === "hotkey" && !editingShortcut && (
            <p role="status" className="text-sm text-muted-foreground">
              {sourceReady ? "Preparing your shortcut…" : "Offline dictation must be ready before you can try it."}
            </p>
          )}
          {phase === 3 && (currentStep === "success" || editingShortcut) && (
            <SuccessStep
              capturedBareModifier={capturedBareModifier}
              holdToTalk={holdToTalk}
              hotkey={hotkey}
              telemetryOptIn={telemetryOptIn}
              isSavingCompletion={isSavingCompletion || savingShortcut || currentStep !== "success"}
              onTelemetryChange={onTelemetryChange}
              onComplete={completeOnboarding}
              onChangeShortcut={() => {
                setEditingShortcut(true);
                handleBack();
              }}
              editor={
                currentStep === "hotkey" && editingShortcut ? (
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
                ) : undefined
              }
            />
          )}
        </div>
      </main>
    </div>
  );
};
