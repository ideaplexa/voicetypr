import { HotkeyStep } from "@/components/onboarding/HotkeyStep";
import { StepDots } from "@/components/onboarding/OnboardingChrome";
import { SourceSheet } from "@/components/onboarding/SourceSheet";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/settings/SettingsButton";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import {
  formatBareModifierLabel,
  NEW_USERS_GET_POLISH,
  type OnboardingDesktopProps,
  type Step,
} from "@/components/onboarding/onboardingTypes";
import { useOnboardingDesktop } from "@/components/onboarding/useOnboardingDesktop";
import { OfflinePreparationStatus } from "@/components/onboarding/OfflinePreparationStatus";
import { PromiseStep } from "@/components/onboarding/PromiseStep";
import { TryStep } from "@/components/onboarding/TryStep";
import { SetupStep } from "@/components/onboarding/SetupStep";
import { TrialStep } from "@/components/onboarding/TrialStep";
import { DoneStep } from "@/components/onboarding/DoneStep";
import { UseStep } from "@/components/onboarding/UseStep";
import { ConsentChoice } from "@/components/onboarding/ConsentChoice";
import { LicenseKeyDialog } from "@/components/LicenseKeyDialog";
import type { LicenseStatus, TrialStartError } from "@/types";

const noop = async () => {};
const megabytes = (bytes: number | undefined) => bytes ? `${Math.round(bytes / (1024 * 1024))} MB` : null;

function trialEnds(status: LicenseStatus | null | undefined) {
  const expires = status?.expires_at ? new Date(status.expires_at) : null;
  if (expires && !Number.isNaN(expires.getTime())) {
    return `Voicetypr is yours until ${expires.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}.`;
  }
  const days = status?.trial_days_left;
  return days !== undefined ? `${days} ${days === 1 ? "day" : "days"} left in your trial.` : null;
}

export const OnboardingDesktop = function OnboardingDesktop(props: OnboardingDesktopProps) {
  const flow = useOnboardingDesktop(props);
  const {
    currentStep,
    goTo,
    sourceType,
    preparation,
    sourceReady,
    confirmSource,
    handleBack,
    handleNext,
    nextDisabled,
    permissions,
    local,
    hotkey,
    holdToTalk,
    capturedBareModifier,
  } = flow;
  const { licenseStatus, onStartTrial, onActivateLicense, onRecheckLicense, onOpenPricing } = props;

  const [aiEnabled, setAiEnabled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [editingShortcut, setEditingShortcut] = useState(false);
  const [heardYou, setHeardYou] = useState(false);
  const [keyDialogOpen, setKeyDialogOpen] = useState(false);
  const [outcome, setOutcome] = useState<{ kind: "trial"; firstName: string } | { kind: "key" } | { kind: "later" } | null>(null);
  const [sawTrial, setSawTrial] = useState(false);
  const [trialError, setTrialError] = useState<TrialStartError | null>(null);
  const [sourceSheet, setSourceSheet] = useState<"closed" | "sources" | "models">("closed");
  const [sourceAtOpen, setSourceAtOpen] = useState(sourceType);
  const openSourceSheet = (view: "sources" | "models") => {
    setSourceAtOpen(sourceType);
    if (view === "models") confirmSource("local");
    setSourceSheet(view);
  };

  useEffect(() => {
    let cancelled = false;
    void invoke<{ enabled?: boolean } | null>("get_ai_settings")
      .then((ai) => { if (!cancelled) setAiEnabled(Boolean(ai?.enabled)); })
      .catch(() => undefined);
    return () => { cancelled = true; };
  }, []);

  if (currentStep === "trial" && !sawTrial) setSawTrial(true);
  // The step count is fixed by the licence as first known, so starting a
  // trial or adding a key never changes it mid-flow.
  const [trialInFlow, setTrialInFlow] = useState<boolean | null>(null);
  if (trialInFlow === null && licenseStatus != null) {
    setTrialInFlow(licenseStatus?.status !== "licensed" && licenseStatus?.status !== "trial");
  }

  // A key activated or a status recheck that finds a license or trial ends the trial step.
  const licensed = licenseStatus?.status === "licensed" || licenseStatus?.status === "trial";
  useEffect(() => {
    if (currentStep === "trial" && licensed) goTo("try");
  }, [currentStep, licensed, goTo]);

  const polishAvailable = NEW_USERS_GET_POLISH || aiEnabled;
  const caps = capturedBareModifier
    ? [formatBareModifierLabel(capturedBareModifier)]
    : shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows");
  const model = preparation.recommended ? local.models[preparation.recommended] : undefined;
  const modelSize = megabytes(model?.size);
  const downloadNote = sourceType === "local" && model && !model.downloaded && modelSize
    ? `The dictation engine (${modelSize}) downloads once, in the background.`
    : null;
  const flowSteps: Step[] = ["promise", ...(trialInFlow ? (["trial"] as Step[]) : []), "try", "setup", "use", "done"];
  const dotIndex = Math.max(0, flowSteps.indexOf(currentStep));
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    try { await action(); } finally { setBusy(false); }
  };
  const upgrade = () => {
    void (onOpenPricing ?? noop)();
    setKeyDialogOpen(true);
  };
  const consent = <ConsentChoice checked={flow.telemetryOptIn} onChange={flow.onTelemetryChange} />;
  const preparing = sourceType === "local" ? (
    <OfflinePreparationStatus
      verifying={Boolean(preparation.name && local.verifyingModels.has(preparation.name))}
      progress={preparation.name ? local.downloadProgress[preparation.name] : undefined}
      size={megabytes(preparation.name ? local.models[preparation.name]?.size : undefined)}
      error={preparation.name ? local.downloadErrors[preparation.name] : undefined}
      onRetry={() => { if (preparation.name) local.onDownload(preparation.name); }}
      onChoose={() => openSourceSheet("models")}
    />
  ) : null;

  const languageWarning = preparation.warningModel ? (
    <div role="alert" className="flex flex-col items-center gap-3 text-sm">
      <p>This model does not support your chosen language. Download it without selecting it?</p>
      <div className="flex gap-2">
        <Button variant="outline" onClick={preparation.dismissWarning}>Cancel</Button>
        <Button onClick={preparation.confirmDownload}>Download anyway</Button>
      </div>
    </div>
  ) : null;

  const doneTitle = outcome?.kind === "trial"
    ? outcome.firstName ? `You’re in, ${outcome.firstName}` : "You’re in"
    : outcome?.kind === "key" ? "License active. Voicetypr is yours."
    : licenseStatus?.status === "licensed" && !sawTrial ? `Welcome back. Your license is active on this ${isMacOS ? "Mac" : "PC"}.`
    : "You’re set";
  const doneDetail = licenseStatus?.status === "trial" ? trialEnds(licenseStatus)
    : licenseStatus?.status === "licensed" ? null
    : "Dictation is paused until you start your trial or add a license key under Settings → License.";

  return (
    <div className="flex min-h-screen flex-col items-center bg-background pt-[44px] text-foreground">
      {trialInFlow === null ? <div aria-hidden="true" className="h-[6px]" /> : <StepDots currentIndex={dotIndex} total={flowSteps.length} />}
      <main className="flex w-full flex-1 items-center justify-center px-6 py-6">
        <div className="flex w-full max-w-[640px] flex-col items-center gap-[22px]">
          {currentStep === "promise" && (
            <PromiseStep
              polishAvailable={polishAvailable}
              downloadNote={downloadNote}
              nextLine={licenseStatus?.status === "licensed" ? "Your license is active. Say anything."
                : licenseStatus?.status === "trial" ? "Your free week is running. Say anything."
                : licenseStatus?.status === "expired" ? "Upgrade or add your key, then say anything."
                : "Start your free week, then say anything."}
              busy={busy || preparation.savingLanguage}
              onBegin={() => void run(handleNext)}
            />
          )}

          {currentStep === "trial" && (
            <TrialStep
              trialEnded={licenseStatus?.status === "expired"}
              localDictation={sourceType === "local"}
              onStartTrial={async (name, email) => {
                const error = await (onStartTrial ?? (async () => "trial_failed" as const))(name, email);
                setTrialError(error);
                if (!error) {
                  setOutcome({ kind: "trial", firstName: name.split(/\s+/)[0] ?? "" });
                  goTo("try");
                }
                return error;
              }}
              onUpgrade={upgrade}
              onHaveKey={() => setKeyDialogOpen(true)}
              onBack={handleBack}
              onDecideLater={() => {
                setOutcome({ kind: "later" });
                goTo("try");
              }}
              consent={consent}
            />
          )}

          {currentStep === "try" && (
            <TryStep
              caps={caps}
              holdToTalk={holdToTalk}
              polishAvailable={polishAvailable}
              engineReady={sourceReady}
              access={licensed || outcome?.kind === "trial" || outcome?.kind === "key" ? "ok"
                : licenseStatus?.status === "expired" ? "ended"
                : trialError === "trial_unreachable" ? "offline" : "none"}
              onStartTrial={() => goTo("trial")}
              onHaveKey={() => setKeyDialogOpen(true)}
              preparing={preparing}
              microphone={permissions.microphone.status}
              onAllowMicrophone={flow.ensureMicrophone}
              onBack={handleBack}
              localSource={sourceType === "local"}
              onUseSomethingElse={() => openSourceSheet("sources")}
              onNext={() => void run(handleNext)}
              onWorked={() => setHeardYou(true)}
            />
          )}

          {currentStep === "setup" && (
            <SetupStep
              caps={caps}
              holdToTalk={holdToTalk}
              heardYou={heardYou}
              permissions={permissions}
              checkingPermissions={flow.checkingPermissions}
              isRequestingPermission={flow.isRequestingPermission}
              onCheck={flow.checkSinglePermission}
              onRequest={flow.requestPermission}
              editor={editingShortcut ? (
                <HotkeyStep
                  hotkey={hotkey}
                  holdToTalk={holdToTalk}
                  capturedBareModifier={capturedBareModifier}
                  onHotkeyChange={flow.onHotkeyChange}
                  onEditingChange={flow.onEditingChange}
                  onBareModifier={flow.onBareModifier}
                  onHoldToTalkChange={flow.onHoldToTalkChange}
                  onBack={() => setEditingShortcut(false)}
                  onNext={() => void run(async () => {
                    await flow.saveHotkeySettings();
                    setEditingShortcut(false);
                  })}
                  nextDisabled={flow.isEditingHotkey || busy}
                />
              ) : null}
              onChangeShortcut={() => setEditingShortcut(true)}
              onBack={handleBack}
              onSkipAccessibility={() => {
                flow.skipAccessibility();
                void run(handleNext);
              }}
              onNext={() => void run(handleNext)}
              nextDisabled={nextDisabled}
              saving={busy}
            />
          )}

          {currentStep === "use" && (
            <UseStep
              polishAvailable={aiEnabled}
              saving={busy}
              onBack={handleBack}
              onChoose={(style) => void run(() => flow.chooseStartingStyle(style, aiEnabled))}
            />
          )}

          {currentStep === "done" && (
            <DoneStep
              title={doneTitle}
              detail={doneDetail}
              caps={caps}
              holdToTalk={holdToTalk}
              copyOnly={isMacOS && (flow.accessibilitySkipped || permissions.accessibility.status === "denied" ||
                permissions.accessibility.status === "not_determined")}
              consent={sawTrial ? null : consent}
              saving={flow.isSavingCompletion}
              onFinish={() => void flow.completeOnboarding()}
            />
          )}

          {sourceSheet === "closed" && languageWarning}
        </div>
      </main>
      <SourceSheet
        open={sourceSheet !== "closed"}
        showModels={sourceSheet === "models"}
        sourceType={sourceType}
        modelSize={modelSize}
        language={preparation.language}
        local={local}
        cloud={flow.cloud}
        remote={flow.remote}
        onConfirmSource={confirmSource}
        onLanguageChange={(value) => void preparation.changeLanguage(value)}
        warning={languageWarning}
        onDone={() => {
          setSourceSheet("closed");
          void flow.finishSource(sourceAtOpen !== "local");
        }}
      />
      <LicenseKeyDialog
        open={keyDialogOpen}
        onOpenChange={setKeyDialogOpen}
        onActivate={async (key) => {
          const error = await (onActivateLicense ?? (async () => "Couldn’t activate the key. Try again."))(key);
          if (!error) {
            setOutcome({ kind: "key" });
            if (currentStep === "trial") goTo("try");
          }
          return error;
        }}
        onRecheck={onRecheckLicense ?? noop}
      />
    </div>
  );
};
