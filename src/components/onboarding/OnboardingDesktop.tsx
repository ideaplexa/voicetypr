import { HotkeyStep } from "@/components/onboarding/HotkeyStep";
import { StepDots } from "@/components/onboarding/OnboardingChrome";
import { SourceStep } from "@/components/onboarding/SourceStep";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/settings/SettingsButton";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { ReadinessLocalPanel } from "@/components/onboarding/ReadinessLocalPanel";
import { ReadinessCloudPanel } from "@/components/onboarding/ReadinessCloudPanel";
import { ReadinessRemotePanel } from "@/components/onboarding/ReadinessRemotePanel";
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
import { useSettings } from "@/contexts/SettingsContext";
import type { LicenseStatus, TrialStartError } from "@/types";

const noop = async () => {};

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
    openModelChoices,
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
  const { settings, updateSettings } = useSettings();
  const builtinPolishSetting = settings?.builtin_polish_enabled;

  // Only a new install starts with built-in Polish on. A user who finished onboarding
  // before (onboarding reopened to recover a missing model) keeps it off until they choose.
  const newInstall = settings?.onboarding_completed !== true;
  useEffect(() => {
    if (!NEW_USERS_GET_POLISH || props.previewStep || !settings || !newInstall || builtinPolishSetting != null) return;
    void Promise.resolve(updateSettings({ builtin_polish_enabled: true })).catch(() => undefined);
  }, [settings, newInstall, builtinPolishSetting, updateSettings, props.previewStep]);

  useEffect(() => {
    let cancelled = false;
    void invoke<{ enabled?: boolean } | null>("get_ai_settings")
      .then((ai) => { if (!cancelled) setAiEnabled(Boolean(ai?.enabled)); })
      .catch(() => undefined);
    return () => { cancelled = true; };
  }, []);

  if (currentStep === "trial" && !sawTrial) setSawTrial(true);

  // A key activated or a status recheck that finds a license or trial ends the trial step.
  const licensed = licenseStatus?.status === "licensed" || licenseStatus?.status === "trial";
  useEffect(() => {
    if (currentStep === "trial" && licensed) goTo("try");
  }, [currentStep, licensed, goTo]);

  const builtinPolish = !aiEnabled && NEW_USERS_GET_POLISH &&
    (builtinPolishSetting === true || (newInstall && builtinPolishSetting == null));
  const polishAvailable = aiEnabled || builtinPolish;
  const caps = capturedBareModifier
    ? [formatBareModifierLabel(capturedBareModifier)]
    : shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows");
  const model = preparation.recommended ? local.models[preparation.recommended] : undefined;
  const modelSize = model?.size ? `${Math.round(model.size / (1024 * 1024))} MB` : null;
  const downloadNote = sourceType === "local" && model && !model.downloaded && modelSize
    ? `Apart from a one-time download of the dictation engine (${modelSize}).`
    : null;
  const flowSteps: Step[] = ["promise", ...(sawTrial || !licensed ? (["trial"] as Step[]) : []), "try", "setup", "use", "done"];
  const dotIndex = Math.max(0, flowSteps.indexOf(currentStep === "source" || currentStep === "readiness" ? "promise" : currentStep));
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
      ready={local.localReady}
      hideWhenReady
      verifying={Boolean(preparation.name && local.verifyingModels.has(preparation.name))}
      progress={preparation.name ? local.downloadProgress[preparation.name] : undefined}
      failed={Boolean(preparation.name && local.downloadErrors[preparation.name])}
      onCancel={() => { if (preparation.name) local.onCancelDownload(preparation.name); }}
      onRetry={() => { if (preparation.name) local.onDownload(preparation.name); }}
      onChoose={openModelChoices}
    />
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
      <StepDots currentIndex={dotIndex} total={flowSteps.length} />
      <main className="flex w-full flex-1 items-center justify-center px-6 py-6">
        <div className="flex w-full max-w-[640px] flex-col items-center gap-[22px]">
          {currentStep === "promise" && (
            <PromiseStep
              polishAvailable={polishAvailable}
              downloadNote={downloadNote}
              busy={busy || preparation.savingLanguage}
              onBegin={() => void run(handleNext)}
              onChooseEngine={() => goTo("source")}
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

          {(currentStep === "source" || currentStep === "readiness") && (
            <>
              <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
                Where should your voice become text?
              </h2>
              <p className="text-center text-sm leading-[normal] text-muted-foreground">
                You can change this any time. Most people start on this {isMacOS ? "Mac" : "PC"}.
              </p>
              {currentStep === "source" ? (
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
                  {sourceType === "cloud" && <ReadinessCloudPanel {...flow.cloud} />}
                  {sourceType === "remote" && <ReadinessRemotePanel {...flow.remote} />}
                </div>
              )}
              <div className="flex gap-[10px]">
                <Button variant="outline" onClick={handleBack}>Back</Button>
                <Button disabled={nextDisabled || busy} onClick={() => void run(handleNext)}>Continue</Button>
              </div>
              {currentStep === "source" && sourceType === "local" && (
                <Button variant="ghost" onClick={openModelChoices}>Choose a different model</Button>
              )}
            </>
          )}

          {currentStep === "try" && (
            <TryStep
              caps={caps}
              holdToTalk={holdToTalk}
              polishAvailable={polishAvailable}
              builtinPolish={builtinPolish}
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
              onChooseEngine={() => goTo("source")}
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
              polishAvailable={polishAvailable}
              saving={busy}
              onBack={handleBack}
              onChoose={(style) => void run(() => flow.chooseStartingStyle(style, polishAvailable))}
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

          {preparation.warningModel && (
            <div role="alert" className="flex flex-col items-center gap-3 text-sm">
              <p>This model does not support your chosen language. Download it without selecting it?</p>
              <div className="flex gap-2">
                <Button variant="outline" onClick={preparation.dismissWarning}>Cancel</Button>
                <Button onClick={preparation.confirmDownload}>Download anyway</Button>
              </div>
            </div>
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
            if (currentStep === "trial") goTo("try");
          }
          return error;
        }}
        onRecheck={onRecheckLicense ?? noop}
      />
    </div>
  );
};
