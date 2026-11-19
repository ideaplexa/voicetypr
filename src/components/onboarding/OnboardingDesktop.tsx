import { HotkeyStep } from "@/components/onboarding/HotkeyStep";
import { StepDots } from "@/components/onboarding/OnboardingChrome";
import { VoiceSourceStep } from "@/components/onboarding/VoiceSourceStep";
import { EngineStep } from "@/components/onboarding/EngineStep";
import { LanguageStep } from "@/components/onboarding/LanguageStep";
import { ForkStep } from "@/components/onboarding/ForkStep";
import { useEffect, useState } from "react";
import { Button } from "@/components/settings/SettingsButton";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import {
  formatBareModifierLabel,
  type OnboardingDesktopProps,
} from "@/components/onboarding/onboardingTypes";
import { useOnboardingDesktop } from "@/components/onboarding/useOnboardingDesktop";
import { OfflinePreparationStatus } from "@/components/onboarding/OfflinePreparationStatus";
import { PromiseStep } from "@/components/onboarding/PromiseStep";
import { TryStep } from "@/components/onboarding/TryStep";
import { SetupStep } from "@/components/onboarding/SetupStep";
import { TrialStep } from "@/components/onboarding/TrialStep";
import { BuyStep } from "@/components/onboarding/BuyStep";
import { ResultStep } from "@/components/onboarding/ResultStep";
import type { FirstDictation } from "@/components/onboarding/useFirstDictation";
import { DoneStep } from "@/components/onboarding/DoneStep";
import { PolishStep } from "@/components/onboarding/PolishStep";
import { UseStep } from "@/components/onboarding/UseStep";
import { usePolishStatus } from "@/components/polish/usePolishStatus";
import { ConsentChoice } from "@/components/onboarding/ConsentChoice";
import { LicenseKeyDialog } from "@/components/LicenseKeyDialog";
import { useSettings } from "@/contexts/SettingsContext";
import { toast } from "sonner";
import type { LicenseStatus } from "@/types";

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
  const { licenseStatus, onStartTrial, onActivateLicense, onRecheckLicense, onOpenPricing } = props;
  const [outcome, setOutcome] = useState<{ kind: "trial"; firstName: string } | { kind: "key" } | { kind: "later" } | null>(null);
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

  const [busy, setBusy] = useState(false);
  const [editingShortcut, setEditingShortcut] = useState(false);
  const [keyDialogOpen, setKeyDialogOpen] = useState(false);
  const [sawTrial, setSawTrial] = useState(false);
  const [firstTry, setFirstTry] = useState<FirstDictation | null>(null);
  const { settings, updateSettings } = useSettings();


  const { status: polishStatus } = usePolishStatus(`${licenseStatus?.status}:${currentStep}`);

  if ((currentStep === "buy" || currentStep === "trial") && !sawTrial) setSawTrial(true);
  // A key activated or a status recheck that finds a license or trial goes straight to Done.
  const licensed = licenseStatus?.status === "licensed" || licenseStatus?.status === "trial";
  useEffect(() => {
    if ((currentStep === "buy" || currentStep === "trial") && licensed) goTo("done");
  }, [currentStep, licensed, goTo]);

  // The user's saved choice; a fresh install starts with it on (set in Rust at startup).
  const builtinChosen = settings?.builtin_polish_enabled === true;
  const ownPolishOn = polishStatus?.own_provider_ready === true && polishStatus.own_provider_enabled;
  const builtinOpen = builtinChosen && polishStatus?.own_provider_ready !== true;
  const builtinWillRun = builtinOpen && polishStatus?.builtin === "ready";
  // Before a trial, only the First try is cleaned, on its own small allowance.
  const firstTryPolished = builtinOpen && (polishStatus?.builtin === "ready" || polishStatus?.builtin === "first_try");
  const polishOn = ownPolishOn || builtinWillRun;
  // Before the trial starts the server can't confirm yet, so the promise follows the choice.
  const polishAvailable = polishOn || (builtinChosen && (polishStatus === null || polishStatus.builtin === "no_access" || polishStatus.builtin === "first_try"));
  const caps = capturedBareModifier
    ? [formatBareModifierLabel(capturedBareModifier)]
    : shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows");
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    try { await action(); } finally { setBusy(false); }
  };
  const upgrade = () => {
    void (onOpenPricing ?? noop)();
    setKeyDialogOpen(true);
  };
  const consent = flow.consentRequired
    ? <ConsentChoice checked={flow.telemetryOptIn} onChange={flow.onTelemetryChange} disabled={flow.isSavingCompletion} terms={!sawTrial} />
    : null;
  const [trialPending, setTrialPending] = useState(false);
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
    ["permissions", "shortcut", "polish", "result", "fork", "buy", "trial", "done"].includes(currentStep)
    ? preparing(flow.chooseSourceFromTry, true) : null;

  const warned = preparation.warningModel ? local.models[preparation.warningModel] : undefined;
  const languageWarning = warned && sourceType === "local" ? (
    <div role="alert" className="flex flex-col items-center gap-3 text-sm">
      <p>{warned.display_name} doesn't support your chosen language. Download it without selecting it?</p>
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
      {licenseStatus == null ? <div aria-hidden="true" className="h-[6px]" /> : <StepDots currentIndex={flow.dotIndex} total={flow.dots.length} />}
      <div className="mt-3 flex min-h-5 items-center justify-center">{downloadLine}</div>
      <main className="flex w-full flex-1 items-center justify-center px-6 py-6">
        <div className="flex w-full max-w-[640px] flex-col items-center gap-[22px]">
          {currentStep === "promise" && (
            <PromiseStep
              polishAvailable={polishAvailable}
              nextLine={licenseStatus?.status === "licensed" ? "Your license is active. Say anything."
                : licenseStatus?.status === "trial" ? "Your free week is running. Say anything."
                : licenseStatus?.status === "expired" ? "Set it up, try it, then buy Voicetypr or add your key."
                : "Set it up, then try it in your own voice."}
              busy={busy || preparation.savingLanguage}
              onBegin={() => void run(handleNext)}
            />
          )}

          {currentStep === "buy" && (
            <BuyStep
              trialEnded={licenseStatus?.status === "expired"}
              pending={trialPending}
              onBuy={upgrade}
              onStartTrial={flow.chooseTrial}
              onHaveKey={() => setKeyDialogOpen(true)}
              onBack={handleBack}
              onDecideLater={() => {
                setOutcome({ kind: "later" });
                goTo("done");
              }}
            />
          )}

          {currentStep === "trial" && (
            <TrialStep
              pending={trialPending}
              onStartTrial={async (name, email) => {
                setTrialPending(true);
                const error = await (onStartTrial ?? (async () => "trial_failed" as const))(name, email)
                  .finally(() => setTrialPending(false));
                if (!error) {
                  setOutcome({ kind: "trial", firstName: name.split(/\s+/)[0] ?? "" });
                  goTo("done");
                }
                return error;
              }}
              onBack={handleBack}
            />
          )}

          {currentStep === "source" && (
            <VoiceSourceStep
              sourceType={sourceType}
              busy={busy || nextDisabled}
              onConfirmSource={confirmSource}
              onBack={handleBack}
              onNext={() => void run(handleNext)}
            />
          )}

          {currentStep === "language" && (
            <LanguageStep
              language={preparation.language}
              busy={busy || nextDisabled}
              onLanguageChange={(value) => void preparation.changeLanguage(value)}
              onBack={handleBack}
              onNext={() => void run(handleNext)}
            />
          )}

          {currentStep === "engine" && (
            <EngineStep
              sourceType={sourceType}
              local={local}
              cloud={flow.cloud}
              remote={flow.remote}
              warning={languageWarning}
              busy={busy || nextDisabled}
              onBack={handleBack}
              onNext={() => void run(handleNext)}
            />
          )}

          {currentStep === "try" && (
            <TryStep
              caps={caps}
              holdToTalk={holdToTalk}
              polishAvailable={ownPolishOn || firstTryPolished}
              builtinPolish={firstTryPolished}
              engineReady={sourceReady}
              preparing={sourceType === "local" ? preparing(flow.chooseSourceFromTry) : null}
              microphone={permissions.microphone.status}
              onAllowMicrophone={flow.ensureMicrophone}
              onBack={handleBack}
              localSource={sourceType === "local"}
              onUseSomethingElse={flow.chooseSourceFromTry}
              onResult={(result) => {
                setFirstTry(result);
                goTo("result");
              }}
              onNext={() => {
                setFirstTry(null);
                goTo("result");
              }}
            />
          )}

          {currentStep === "result" && (
            <ResultStep
              result={firstTry}
              polishAvailable={polishAvailable}
              onTryAgain={() => goTo("try")}
              onBack={() => goTo("try")}
              onNext={() => void run(handleNext)}
            />
          )}

          {currentStep === "fork" && (
            <ForkStep
              onDeeper={flow.goDeeper}
              onDoneForNow={flow.leaveFork}
              onBack={handleBack}
            />
          )}

          {(currentStep === "permissions" || currentStep === "shortcut") && (
            <SetupStep
              part={currentStep}
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
                  onBack={() => {
                    flow.onEditingChange(false);
                    setEditingShortcut(false);
                  }}
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
              onBuiltinChange={(on) => void run(() => Promise.resolve(updateSettings({ builtin_polish_enabled: on }))
                .catch(() => { toast.error("Couldn’t save the Polish switch. Try again."); }))}
              saving={busy}
              onBack={handleBack}
              onNext={flow.leavePolish}
            />
          )}

          {currentStep === "use" && (
            <UseStep
              polishOn={polishOn}
              saving={busy}
              onBack={handleBack}
              onChoose={(style) => void run(() => flow.chooseStartingStyle(style, {
                // Set it now unless a provider of the user's own is set up but off; that waits for it to turn on.
                applyNow: ownPolishOn || polishStatus?.own_provider_ready !== true,
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
              consent={consent}
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
            goTo("done");
          }
          return error;
        }}
        onRecheck={onRecheckLicense ?? noop}
      />
    </div>
  );
};
