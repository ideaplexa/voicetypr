import { KeyCaps } from "@/components/KeyCaps";
import { Button } from "@/components/settings/SettingsButton";
import { Card } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import type { PermissionStatus } from "@/components/onboarding/onboardingTypes";
import { useFirstDictation, type FirstDictation } from "@/components/onboarding/useFirstDictation";
import { FounderLine } from "@/components/onboarding/FounderLine";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { CircleCheck, Mic, MicOff, Square } from "lucide-react";
import { useEffect, type ReactNode } from "react";

const MESSY = "um, so let’s move the sync to Thursday, no wait, Friday at three, and I’ll send the notes before";
const PLAIN = "Let’s move the sync to Friday at three, and I’ll send the notes before.";

export function TryStep({
  caps,
  holdToTalk,
  polishAvailable,
  engineReady,
  access,
  onStartTrial,
  onHaveKey,
  preparing,
  microphone,
  onAllowMicrophone,
  onBack,
  localSource,
  onUseSomethingElse,
  onNext,
  onWorked,
}: {
  caps: string[];
  holdToTalk: boolean;
  polishAvailable: boolean;
  engineReady: boolean;
  /** Whether this device may dictate: a trial or license is needed first. */
  access: "ok" | "none" | "offline" | "ended";
  onStartTrial: () => void;
  onHaveKey: () => void;
  /** Download progress for the engine while it is not ready. */
  preparing: ReactNode;
  microphone: PermissionStatus;
  onAllowMicrophone: () => Promise<boolean>;
  onBack: () => void;
  /** Dictation runs on this computer, so "Almost ready" means a download. */
  localSource: boolean;
  /** Opens the choice of the cloud or another computer instead. */
  onUseSomethingElse: () => void;
  onNext: () => void;
  onWorked: (result: FirstDictation) => void;
}) {
  const { listening, working, result, busy, talk, reset } = useFirstDictation(engineReady && access === "ok");
  useEffect(() => {
    if (result) onWorked(result);
  }, [result, onWorked]);
  const micOff = microphone === "denied" || microphone === "restricted";
  // Both panes only when Polish could change the text and, once heard, did.
  const sideBySide = result ? result.polished : polishAvailable;
  const verb = holdToTalk ? "Hold" : "Press";
  const somethingElse = (
    <p className="text-[12.5px] text-muted-foreground">
      Rather use your own cloud key or another computer on your network?{" "}
      <button
        type="button"
        className="font-medium text-foreground underline-offset-4 hover:underline disabled:opacity-50"
        disabled={listening || working}
        onClick={onUseSomethingElse}
      >
        Use something else
      </button>
    </p>
  );

  if (access !== "ok") {
    const blocked = {
      none: ["Dictation starts with your trial", "Start your free 7-day trial or add a license key, and you can say your first sentence straight away.", "Start 7-day trial"],
      offline: ["We couldn’t confirm your trial", "Check your connection and try again. Dictation starts as soon as your trial is confirmed.", "Try again"],
      ended: ["Trial ended", "This device’s free week has been used. Upgrade, or activate a key you already have, to keep dictating.", "Upgrade or add a key"],
    }[access];
    return (
      <section className="flex w-full flex-col items-center gap-[18px] text-center">
        <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">{blocked[0]}</h2>
        <p role="alert" className="max-w-[480px] text-sm leading-6 text-muted-foreground">{blocked[1]}</p>
        <div className="flex flex-wrap items-center justify-center gap-[10px]">
          <Button onClick={onStartTrial}>{blocked[2]}</Button>
          <Button variant="ghost" onClick={onHaveKey}>I have a key</Button>
        </div>
        <div className="flex items-center gap-[10px]">
          <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={onBack}>Back</Button>
          <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={onNext}>Skip for now</Button>
        </div>
        {access === "ended" ? <FounderLine /> : null}
      </section>
    );
  }

  if (!engineReady) {
    return (
      <section className="flex w-full flex-col items-center gap-[18px] text-center">
        <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">Almost ready</h2>
        <p className="max-w-[480px] text-sm leading-8 text-muted-foreground">
          {localSource ? "As soon as it’s ready, " : "As soon as your source is connected, "}
          {verb.toLowerCase()} <KeyCaps caps={caps} size="onboarding" /> and say your sentence.
        </p>
        {preparing}
        {somethingElse}
        <div className="flex flex-wrap justify-center gap-[10px]">
          <Button variant="outline" onClick={onBack}>Back</Button>
          <Button variant="ghost" onClick={onNext}>Skip for now</Button>
        </div>
      </section>
    );
  }

  const start = async () => {
    if (microphone === "not_determined" && !(await onAllowMicrophone())) return;
    await talk();
  };

  return (
    <section className="flex w-full flex-col items-center gap-[18px] text-center">
      <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        {polishAvailable ? "Say something messy" : "Say something"}
      </h2>
      <p className="max-w-[560px] text-base leading-9 text-muted-foreground">
        {verb} <KeyCaps caps={caps} size="onboarding" /> and talk.{" "}
        {holdToTalk ? "Let go when you’re done." : "Press again when you’re done."}
        {polishAvailable ? " Change your mind halfway." : null}
      </p>
      <p className="max-w-[520px] text-[13px] text-muted-foreground">
        Not sure what to say? Read this out: <q>{polishAvailable ? MESSY : PLAIN}</q>
      </p>

      {micOff ? (
        <Card role="alert" className="w-full flex-row items-center gap-3 px-4 py-3 text-left">
          <MicOff className="size-5 shrink-0 text-destructive" />
          <p className="flex-1 text-sm">
            <span className="font-semibold">The microphone is off for Voicetypr</span>, so it can’t hear you yet.
            Switch it on, then come straight back.
          </p>
          <Button size="sm" onClick={() => void onAllowMicrophone()}>
            {isMacOS ? "Open System Settings" : "Open Privacy settings"}
          </Button>
        </Card>
      ) : null}

      <div className={cn("grid w-full gap-3 text-left", sideBySide && "sm:grid-cols-2")}>
        {sideBySide ? (
          <Pane label="You said">
            {result ? result.heard : <span className="text-muted-foreground">{listening ? "Listening…" : "Your words, as spoken."}</span>}
          </Pane>
        ) : null}
        <Pane label={sideBySide ? "Voicetypr cleaned it" : "Voicetypr typed"} highlight={Boolean(result)}>
          {result ? result.typed : (
            <span className="text-muted-foreground">
              {working ? "Transcribing…" : listening ? "Listening…" : "Your words land here."}
            </span>
          )}
        </Pane>
      </div>

      {result ? (
        <p role="status" className="flex items-center gap-2 text-[12.5px] font-medium text-muted-foreground">
          <CircleCheck className="size-4 text-sage" />
          Worked · {result.words} {result.words === 1 ? "word" : "words"}
          {!result.polished ? " · exactly what the engine heard" : null}
        </p>
      ) : null}

      <div className="flex flex-wrap items-center justify-center gap-[10px]">
        {result ? (
          <>
            <Button variant="ghost" onClick={reset}>Try again</Button>
            <Button onClick={onNext}>Continue</Button>
          </>
        ) : (
          <>
            <Button variant="outline" disabled={busy || working || micOff} onClick={() => void start()}>
              {working ? <Spinner /> : listening ? <Square className="size-3.5" /> : <Mic className="size-4" />}
              {listening ? "Stop" : "Or click here to talk"}
            </Button>
            <Button variant="ghost" disabled={listening || working} onClick={onNext}>Skip for now</Button>
          </>
        )}
      </div>
      <div className="flex items-center gap-[10px]">
        <Button variant="ghost" size="sm" className="text-muted-foreground" disabled={listening || working} onClick={onBack}>
          Back
        </Button>
      </div>
      {somethingElse}
    </section>
  );
}

function Pane({ label, highlight, children }: { label: string; highlight?: boolean; children: ReactNode }) {
  return (
    <Card className={cn("min-h-[120px] gap-2 px-[18px] py-4", highlight && "outline-[1.5px] outline-sage -outline-offset-1")}>
      <span className="text-[11.5px] font-medium tracking-wide text-muted-foreground uppercase">{label}</span>
      <p className="text-[16px] leading-[24px] text-foreground" aria-live="polite">{children}</p>
    </Card>
  );
}
