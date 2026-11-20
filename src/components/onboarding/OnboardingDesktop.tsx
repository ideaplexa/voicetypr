import { useEffect, useState, type ReactNode } from "react";
import { CircleCheck, Download } from "lucide-react";
import { toast } from "sonner";
import type { ScreenId } from "@/components/navigation";
import { BuyStep } from "@/components/onboarding/BuyStep";
import { DoneConsent, DoneStep } from "@/components/onboarding/DoneStep";
import { EngineStep } from "@/components/onboarding/EngineStep";
import { ForkStep } from "@/components/onboarding/ForkStep";
import { Frame, FrameChip, QuietNext } from "@/components/onboarding/Frame";
import { HotkeyStep } from "@/components/onboarding/HotkeyStep";
import { KeyStep } from "@/components/onboarding/KeyStep";
import { LanguageStep } from "@/components/onboarding/LanguageStep";
import { CheckFailedStep, TrialEndedStep } from "@/components/onboarding/LicenceStates";
import { PolishStep } from "@/components/onboarding/PolishStep";
import { PromiseStep } from "@/components/onboarding/PromiseStep";
import { ResultStep } from "@/components/onboarding/ResultStep";
import { SetupStep } from "@/components/onboarding/SetupStep";
import { TOUR, TourStep } from "@/components/onboarding/TourStep";
import { TrialFooter, TrialStep } from "@/components/onboarding/TrialStep";
import { TryStep } from "@/components/onboarding/TryStep";
import { UseStep } from "@/components/onboarding/UseStep";
import type { FirstDictation } from "@/components/onboarding/useFirstDictation";
import { VoiceSourceStep } from "@/components/onboarding/VoiceSourceStep";
import { formatBareModifierLabel, type OnboardingDesktopProps, type Step } from "@/components/onboarding/onboardingTypes";
import { useOnboardingDesktop } from "@/components/onboarding/useOnboardingDesktop";
import { usePolishStatus } from "@/components/polish/usePolishStatus";
import { Button } from "@/components/settings/SettingsButton";
import { useSettings } from "@/contexts/SettingsContext";
import { isMacOS } from "@/lib/platform";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import type { LicenseStatus } from "@/types";

const noop = async () => {};
const TOTAL = 13;
/** "N of 13" on each screen's progress bar, as the frames count them. */
const PROGRESS: Partial<Record<Step, number>> = {
  source: 1, language: 2, engine: 3, permissions: 4, shortcut: 5, polish: 6, try: 7, result: 8, fork: 9, use: 9, tour: 9, buy: 10, trial: 11, done: 12,
};
const CHIP_STEPS: Step[] = ["permissions", "shortcut", "polish", "try", "result", "fork", "use"];

function trialLine(status: LicenseStatus | null | undefined) {
  const expires = status?.expires_at ? new Date(status.expires_at) : null;
  if (expires && !Number.isNaN(expires.getTime())) {
    return `Your trial runs until ${expires.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}.`;
  }
  const days = status?.trial_days_left;
  return days !== undefined ? `Your trial has ${days} ${days === 1 ? "day" : "days"} left.` : "Your trial is running.";
}

export const OnboardingDesktop = function OnboardingDesktop(props: OnboardingDesktopProps) {
  const { licenseStatus, licenseCheckFailed, onStartTrial, onActivateLicense, onRecheckLicense, onOpenPricing } = props;
  const [outcome, setOutcome] = useState<{ kind: "trial"; firstName: string } | { kind: "key" } | null>(null);
  const flow = useOnboardingDesktop(props);
  const { currentStep, goTo, sourceType, preparation, sourceReady, local, permissions, hotkey, holdToTalk, capturedBareModifier } = flow;

  const [busy, setBusy] = useState(false);
  const [editingShortcut, setEditingShortcut] = useState(false);
  const [firstTry, setFirstTry] = useState<FirstDictation | null>(null);
  const [tourIndex, setTourIndex] = useState(0);
  const [remembered, setRemembered] = useState<ScreenId | null>(null);
  const [enteringKey, setEnteringKey] = useState(false);
  const [trialPending, setTrialPending] = useState(false);
  const [trialUnreachable, setTrialUnreachable] = useState(false);
  const [rechecking, setRechecking] = useState(false);
  const [startedWithAccess] = useState(() => licenseStatus?.status === "licensed" || licenseStatus?.status === "trial");

  const { settings, updateSettings } = useSettings();
  const { status: polishStatus } = usePolishStatus(`${licenseStatus?.status}:${currentStep}`);

  // A key activated or a recheck that finds a license or trial goes straight to Done.
  const licensed = licenseStatus?.status === "licensed" || licenseStatus?.status === "trial";
  useEffect(() => {
    if ((currentStep === "buy" || currentStep === "trial") && licensed) goTo("done");
  }, [currentStep, licensed, goTo]);

  // The user's saved choice; a fresh install starts with it on (set in Rust at startup).
  const builtinChosen = settings?.builtin_polish_enabled === true;
  const ownReady = polishStatus?.own_provider_ready === true;
  const ownPolishOn = ownReady && polishStatus.own_provider_enabled;
  // Polish is switched on, though the server may not run it before a trial.
  const polishChosen = ownReady ? ownPolishOn : builtinChosen;
  const caps = capturedBareModifier
    ? [formatBareModifierLabel(capturedBareModifier)]
    : shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows");
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    try { await action(); } finally { setBusy(false); }
  };
  const next = () => void run(flow.handleNext);
  const buy = () => {
    void (onOpenPricing ?? noop)();
    setEnteringKey(true);
  };

  const downloading = preparation.name ? local.models[preparation.name] : undefined;
  const progress = preparation.name ? local.downloadProgress[preparation.name] : undefined;
  const downloadError = preparation.name ? local.downloadErrors[preparation.name] : undefined;
  const chip = sourceType === "local" && downloading && CHIP_STEPS.includes(currentStep) ? (
    local.localReady ? (
      <FrameChip icon={<CircleCheck aria-hidden="true" className="size-[13px] text-sage" />}>{downloading.display_name} is ready</FrameChip>
    ) : (
      <FrameChip icon={<Download aria-hidden="true" className="size-[13px] text-sage" />}>
        {downloading.display_name} · {downloadError ? "download stopped" : `${Math.round(progress ?? 0)}%`}
      </FrameChip>
    )
  ) : null;

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

  const done = outcome?.kind === "trial"
    ? { title: outcome.firstName ? `You’re set, ${outcome.firstName}` : "You’re set", line: trialLine(licenseStatus) }
    : outcome?.kind === "key" ? { title: "Voicetypr is yours", line: "Your licence is active." }
    : startedWithAccess ? { title: "Welcome back", line: licenseStatus?.status === "trial" ? trialLine(licenseStatus) : "Your licence is active." }
    : { title: "You’re set", line: licenseStatus?.status === "trial" ? trialLine(licenseStatus) : "Your licence is active." };

  const paid = currentStep === "buy" || currentStep === "trial";
  const checkFailed = paid && Boolean(licenseCheckFailed) && !licensed;
  const trialEnded = paid && !checkFailed && licenseStatus?.status === "expired";

  let frame: { back?: () => void; right?: ReactNode; footer?: ReactNode; progress: boolean; body: ReactNode };
  if (enteringKey) {
    frame = {
      progress: false,
      back: () => setEnteringKey(false),
      body: (
        <KeyStep
          onBuy={() => void (onOpenPricing ?? noop)()}
          onActivate={async (key) => {
            const error = await (onActivateLicense ?? (async () => "Couldn’t activate the key. Try again."))(key);
            if (!error) {
              setOutcome({ kind: "key" });
              setEnteringKey(false);
              goTo("done");
            }
            return error;
          }}
        />
      ),
    };
  } else if (checkFailed) {
    frame = {
      progress: false,
      body: (
        <CheckFailedStep
          checking={rechecking}
          onRecheck={() => {
            setRechecking(true);
            void (onRecheckLicense ?? noop)().finally(() => setRechecking(false));
          }}
        />
      ),
    };
  } else if (trialEnded) {
    frame = { progress: false, body: <TrialEndedStep onBuy={buy} onHaveKey={() => setEnteringKey(true)} /> };
  } else {
    frame = { progress: currentStep !== "promise", back: () => void flow.handleBack(), body: null };
    switch (currentStep) {
      case "promise":
        frame.back = undefined;
        frame.body = <PromiseStep busy={busy || preparation.savingLanguage} onBegin={next} />;
        break;
      case "source":
        frame.body = (
          <VoiceSourceStep sourceType={sourceType} busy={busy || flow.nextDisabled} onConfirmSource={flow.confirmSource} onNext={next} />
        );
        break;
      case "language":
        frame.body = (
          <LanguageStep
            language={preparation.language}
            busy={busy || flow.nextDisabled}
            onLanguageChange={(value) => void preparation.changeLanguage(value)}
            onNext={next}
          />
        );
        break;
      case "engine":
        frame.body = (
          <EngineStep
            sourceType={sourceType}
            local={local}
            cloud={flow.cloud}
            remote={flow.remote}
            warning={languageWarning}
            ready={sourceType === "local" || sourceReady}
            busy={busy || flow.nextDisabled}
            onNext={next}
          />
        );
        break;
      case "permissions":
      case "shortcut":
        if (currentStep === "shortcut" && editingShortcut) frame.back = undefined;
        frame.body = (
          <SetupStep
            part={currentStep}
            caps={caps}
            holdToTalk={holdToTalk}
            onHoldToTalkChange={flow.onHoldToTalkChange}
            permissions={permissions}
            isRequestingPermission={flow.isRequestingPermission}
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
            onSkipAccessibility={next}
            onNext={next}
            nextDisabled={flow.nextDisabled}
            saving={busy}
          />
        );
        break;
      case "polish":
        frame.body = (
          <PolishStep
            status={polishStatus}
            builtinChosen={builtinChosen}
            onBuiltinChange={(on) => void run(() => Promise.resolve(updateSettings({ builtin_polish_enabled: on }))
              .catch(() => { toast.error("Couldn’t save the Polish switch. Try again."); }))}
            saving={busy}
            ownProviderLater={remembered === "polish"}
            onOwnProvider={() => setRemembered("polish")}
            onNext={() => goTo("try")}
          />
        );
        break;
      case "try": {
        // A try the user already made stays theirs: Buy shows their own words.
        const skip = () => goTo("fork");
        frame.right = <QuietNext onClick={skip}>Skip for now</QuietNext>;
        frame.body = (
          <TryStep
            caps={caps}
            holdToTalk={holdToTalk}
            engineReady={sourceReady}
            download={sourceType === "local" && downloading && !local.localReady ? {
              model: downloading.display_name,
              progress,
              size: downloading.size,
              error: downloadError,
              onRetry: () => { if (preparation.name) local.onDownload(preparation.name); },
            } : null}
            microphone={permissions.microphone.status}
            clickToTalk={isMacOS && permissions.accessibility.status !== "granted"}
            onAllowMicrophone={flow.ensureMicrophone}
            onResult={(result) => {
              setFirstTry(result);
              goTo("result");
            }}
            onSkip={skip}
          />
        );
        break;
      }
      case "result":
        frame.back = () => goTo("try");
        frame.body = firstTry ? (
          <ResultStep result={firstTry} polishOn={polishChosen} onTryAgain={() => goTo("try")} onNext={() => goTo("fork")} />
        ) : null;
        break;
      case "fork":
        frame.back = () => goTo(firstTry ? "result" : "try");
        frame.body = (
          <ForkStep
            onDeeper={() => {
              setTourIndex(0);
              flow.leaveFork(true, polishChosen);
            }}
            onDoneForNow={() => flow.leaveFork(false, false)}
          />
        );
        break;
      case "use":
        frame.body = (
          <UseStep
            saving={busy}
            onChoose={(style) => void run(() => flow.chooseStartingStyle(style, {
              // Set it now unless a provider of the user's own is set up but off; that waits for it to turn on.
              applyNow: ownPolishOn || !ownReady,
              keepForOwnProvider: !ownPolishOn,
            }))}
          />
        );
        break;
      case "tour":
        frame.back = () => (tourIndex > 0 ? setTourIndex(tourIndex - 1) : void flow.handleBack());
        frame.right = <QuietNext onClick={() => void run(flow.handleNext)}>Skip the tour</QuietNext>;
        frame.body = (
          <TourStep
            index={tourIndex}
            remembered={remembered}
            onOpen={setRemembered}
            onNext={() => (tourIndex < TOUR.length - 1 ? setTourIndex(tourIndex + 1) : next())}
          />
        );
        break;
      case "buy":
        frame.body = (
          <BuyStep
            result={firstTry}
            source={sourceType}
            pending={trialPending}
            onBuy={buy}
            onStartTrial={flow.chooseTrial}
            onHaveKey={() => setEnteringKey(true)}
          />
        );
        break;
      case "trial":
        if (trialUnreachable) frame.back = () => setTrialUnreachable(false);
        else frame.footer = <TrialFooter />;
        frame.body = (
          <TrialStep
            pending={trialPending}
            unreachable={trialUnreachable}
            onUnreachableChange={setTrialUnreachable}
            onHaveKey={() => {
              setTrialUnreachable(false);
              setEnteringKey(true);
            }}
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
          />
        );
        break;
      case "done":
        frame.back = undefined;
        frame.footer = flow.consentRequired ? (
          <DoneConsent checked={flow.telemetryOptIn} disabled={flow.isSavingCompletion} onChange={flow.onTelemetryChange} />
        ) : null;
        frame.body = (
          <DoneStep
            title={done.title}
            line={done.line}
            caps={caps}
            holdToTalk={holdToTalk}
            saving={flow.isSavingCompletion}
            onFinish={() => void flow.completeOnboarding(remembered ?? undefined)}
            onSeeCapabilities={() => void flow.completeOnboarding("capabilities")}
          />
        );
        break;
    }
  }

  const step = PROGRESS[currentStep];
  return (
    <Frame
      progress={frame.progress && step ? { done: step, total: TOTAL } : null}
      chip={enteringKey || checkFailed || trialEnded ? null : chip}
      onBack={frame.back}
      backDisabled={busy || trialPending || flow.selectingRemote}
      right={frame.right}
      footer={frame.footer}
    >
      {frame.body}
    </Frame>
  );
};

