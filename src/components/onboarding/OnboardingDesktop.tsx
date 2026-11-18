import { HotkeyStep } from "@/components/onboarding/HotkeyStep";
import { StepDots } from "@/components/onboarding/OnboardingChrome";
import { VoiceSourceStep } from "@/components/onboarding/VoiceSourceStep";
import { useEffect, useState } from "react";
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
import { PolishStep } from "@/components/onboarding/PolishStep";
import { usePolishStatus } from "@/components/polish/usePolishStatus";
import type { FirstDictation } from "@/components/onboarding/useFirstDictation";
import { ConsentChoice } from "@/components/onboarding/ConsentChoice";
import { LicenseKeyDialog } from "@/components/LicenseKeyDialog";
import { useSettings } from "@/contexts/SettingsContext";
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

  const [busy, setBusy] = useState(false);
  const [editingShortcut, setEditingShortcut] = useState(false);
  const [firstTry, setFirstTry] = useState<FirstDictation | null>(null);
  const [keyDialogOpen, setKeyDialogOpen] = useState(false);
  const [outcome, setOutcome] = useState<{ kind: "trial"; firstName: string } | { kind: "key" } | { kind: "later" } | null>(null);
  const [sawTrial, setSawTrial] = useState(false);
  const [trialError, setTrialError] = useState<TrialStartError | null>(null);
  const { settings, updateSettings } = useSettings();
  const builtinPolishSetting = settings?.builtin_polish_enabled;

  // Only a new install starts with built-in Polish on. A user who finished onboarding
  // before (onboarding reopened to recover a missing model) keeps it off until they choose.
  const newInstall = settings?.onboarding_completed !== true;
  useEffect(() => {
    if (!NEW_USERS_GET_POLISH || props.previewStep || !settings || !newInstall || builtinPolishSetting != null) return;
    void Promise.resolve(updateSettings({ builtin_polish_enabled: true })).catch(() => undefined);
  }, [settings, newInstall, builtinPolishSetting, updateSettings, props.previewStep]);
  const [modelsFirst, setModelsFirst] = useState(false);
  const toSource = (models: boolean) => {
    setModelsFirst(models);
    flow.chooseSourceFromTry();
  };

  const { status: polishStatus } = usePolishStatus(`${licenseStatus?.status}:${currentStep === "polish"}`);

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
    if (currentStep === "trial" && licensed) goTo("source");
  }, [currentStep, licensed, goTo]);

  // Built-in Polish is the user's saved choice, or on for a new install that hasn't chosen yet.
  const builtinChosen = builtinPolishSetting === true || (NEW_USERS_GET_POLISH && newInstall && builtinPolishSetting == null);
  const ownPolishOn = polishStatus?.own_provider_ready === true && polishStatus.own_provider_enabled;
  const builtinWillRun = builtinChosen && polishStatus?.own_provider_ready !== true && polishStatus?.builtin === "ready";
  const polishOn = ownPolishOn || builtinWillRun;
  // Before the trial starts the server can't confirm yet, so the promise follows the choice.
  const polishAvailable = polishOn || (builtinChosen && (polishStatus === null || polishStatus.builtin === "no_access"));
  const caps = capturedBareModifier
    ? [formatBareModifierLabel(capturedBareModifier)]
    : shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows");
  const model = preparation.name ? local.models[preparation.name] : undefined;
  const modelSize = megabytes(model?.size);
  const flowSteps: Step[] = ["promise", ...(trialInFlow ? (["trial"] as Step[]) : []), "source", "setup", "try", "polish", "done"];
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
  const preparing = (onChoose: () => void, compact = false) => (
    <OfflinePreparationStatus
      compact={compact}
      ready={local.localReady}
      verifying={Boolean(preparation.name && local.verifyingModels.has(preparation.name))}
      progress={preparation.name ? local.downloadProgress[preparation.name] : undefined}
      size={megabytes(preparation.name ? local.models[preparation.name]?.size : undefined)}
      error={preparation.name ? local.downloadErrors[preparation.name] : undefined}
      onRetry={() => { if (preparation.name) local.onDownload(preparation.name); }}
      onChoose={onChoose}
    />
  );
  const downloadLine = sourceType === "local" && !local.localReady &&
    (currentStep === "setup" || currentStep === "polish" || currentStep === "done")
    ? preparing(() => toSource(true), true) : null;

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
      <div className="mt-3 flex min-h-5 items-center justify-center">{downloadLine}</div>
      <main className="flex w-full flex-1 items-center justify-center px-6 py-6">
        <div className="flex w-full max-w-[640px] flex-col items-center gap-[22px]">
          {currentStep === "promise" && (
            <PromiseStep
              polishAvailable={polishAvailable}
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
                  goTo("source");
                }
                return error;
              }}
              onUpgrade={upgrade}
              onHaveKey={() => setKeyDialogOpen(true)}
              onBack={handleBack}
              onDecideLater={() => {
                setOutcome({ kind: "later" });
                goTo("source");
              }}
              consent={consent}
            />
          )}

          {currentStep === "source" && (
            <VoiceSourceStep
              sourceType={sourceType}
              modelName={model?.display_name ?? null}
              modelSize={modelSize}
              language={preparation.language}
              modelsOpenAtStart={modelsFirst}
              preparing={(onChoose) => preparing(onChoose)}
              local={local}
              cloud={flow.cloud}
              remote={flow.remote}
              warning={languageWarning}
              busy={busy || nextDisabled}
              onConfirmSource={confirmSource}
              onLanguageChange={(value) => void preparation.changeLanguage(value)}
              onBack={handleBack}
              onNext={() => void run(handleNext)}
            />
          )}

          {currentStep === "try" && (
            <TryStep
              caps={caps}
              holdToTalk={holdToTalk}
              polishAvailable={polishAvailable}
              builtinPolish={builtinWillRun}
              engineReady={sourceReady}
              access={licensed || outcome?.kind === "trial" || outcome?.kind === "key" ? "ok"
                : licenseStatus?.status === "expired" ? "ended"
                : trialError === "trial_unreachable" ? "offline" : "none"}
              onStartTrial={() => goTo("trial")}
              onHaveKey={() => setKeyDialogOpen(true)}
              preparing={sourceType === "local" ? preparing(() => toSource(true)) : null}
              microphone={permissions.microphone.status}
              onAllowMicrophone={flow.ensureMicrophone}
              onBack={handleBack}
              localSource={sourceType === "local"}
              onUseSomethingElse={() => toSource(false)}
              onNext={() => void run(handleNext)}
              onWorked={setFirstTry}
            />
          )}

          {currentStep === "setup" && (
            <SetupStep
              caps={caps}
              holdToTalk={holdToTalk}
              onHoldToTalkChange={flow.onHoldToTalkChange}
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
              onSkip={flow.skipSetup}
              onSkipAccessibility={() => {
                flow.skipAccessibility();
                void run(handleNext);
              }}
              onNext={() => void run(handleNext)}
              nextDisabled={nextDisabled}
              saving={busy}
            />
          )}

          {currentStep === "polish" && (
            <PolishStep
              status={polishStatus}
              builtinChosen={builtinChosen}
              onBuiltinChange={(on) => void Promise.resolve(updateSettings({ builtin_polish_enabled: on })).catch(() => undefined)}
              firstTry={firstTry}
              saving={busy}
              onBack={handleBack}
              onChoose={(style) => void run(() => flow.chooseStartingStyle(style, {
                applyNow: polishOn || polishStatus?.builtin === "ready",
                keepForOwnProvider: !ownPolishOn,
              }))}
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
              onBack={handleBack}
              onFinish={() => void flow.completeOnboarding()}
            />
          )}

        </div>
      </main>
      <LicenseKeyDialog
        open={keyDialogOpen}
        onOpenChange={setKeyDialogOpen}
        onActivate={async (key) => {
          const error = await (onActivateLicense ?? (async () => "Couldn’t activate the key. Try again."))(key);
          if (!error) {
            setOutcome({ kind: "key" });
            if (currentStep === "trial") goTo("source");
          }
          return error;
        }}
        onRecheck={onRecheckLicense ?? noop}
      />
    </div>
  );
};
