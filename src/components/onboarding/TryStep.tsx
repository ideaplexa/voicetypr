import { KeyCaps } from "@/components/KeyCaps";
import { Band } from "@/components/onboarding/OnboardingChrome";
import { Button } from "@/components/settings/SettingsButton";
import { Card } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import type { PermissionStatus } from "@/components/onboarding/onboardingTypes";
import { useFirstDictation, type FirstDictation } from "@/components/onboarding/useFirstDictation";
import { isMacOS } from "@/lib/platform";
import { Mic, MicOff, Square } from "lucide-react";
import type { ReactNode } from "react";

export const MESSY = "um, so let’s move the sync to Thursday, no wait, Friday at three, and I’ll send the notes before";
export const PLAIN = "Let’s move the sync to Friday at three, and I’ll send the notes before.";

/** The First try: a real dictation shown only in this window. */
export function TryStep({
  caps,
  holdToTalk,
  polishAvailable,
  builtinPolish = false,
  engineReady,
  preparing,
  microphone,
  onAllowMicrophone,
  onBack,
  localSource,
  onUseSomethingElse,
  onResult,
  onNext,
}: {
  caps: string[];
  holdToTalk: boolean;
  polishAvailable: boolean;
  /** This try is cleaned on Voicetypr's server rather than the user's own provider. */
  builtinPolish?: boolean;
  engineReady: boolean;
  /** Download progress for the engine while it is not ready. */
  preparing: ReactNode;
  microphone: PermissionStatus;
  onAllowMicrophone: () => Promise<boolean>;
  onBack: () => void;
  /** Dictation runs on this computer, so "Almost ready" means a download. */
  localSource: boolean;
  /** Opens the choice of the cloud or another computer instead. */
  onUseSomethingElse: () => void;
  onResult: (result: FirstDictation) => void;
  /** Moves on without a dictation of their own. */
  onNext: () => void;
}) {
  const { listening, working, allowance, busy, talk } = useFirstDictation(engineReady, holdToTalk, onResult);
  const micOff = microphone === "denied" || microphone === "restricted";
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
  if (allowance?.kind === "usedUp") {
    return (
      <section className="flex w-full flex-col items-center gap-[18px] text-center">
        <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">Your first tries are used up</h2>
        <p className="max-w-[480px] text-sm leading-6 text-muted-foreground">
          This device has had its first tries. Here’s what a dictation looks like instead.
        </p>
        <div className="flex flex-wrap justify-center gap-[10px]">
          <Button variant="outline" onClick={onBack}>Back</Button>
          <Button onClick={onNext}>Continue</Button>
        </div>
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
    if (!listening && microphone === "not_determined" && !(await onAllowMicrophone())) return;
    await talk();
  };
  return (
    <section className="flex w-full flex-col items-center gap-[18px] text-center">
      <Band>
        <span className="flex items-center gap-3 text-sm font-medium text-sage">
          {verb}
          <KeyCaps caps={caps} size="lg" />
        </span>
      </Band>
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
      <p className="max-w-[520px] text-[12.5px] text-muted-foreground">
        Your words stay in this window: nothing is typed into other apps or saved.
        {builtinPolish ? " Polish sends them to Voicetypr’s server to clean them up. Voicetypr’s server keeps nothing." : null}
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
      <p role="status" aria-live="polite" className="min-h-5 text-sm text-muted-foreground">
        {working ? "Transcribing…" : listening ? "Listening…" : null}
      </p>
      <div className="flex flex-wrap items-center justify-center gap-[10px]">
        <Button variant="outline" disabled={busy || working || micOff} onClick={() => void start()}>
          {working ? <Spinner /> : listening ? <Square className="size-3.5" /> : <Mic className="size-4" />}
          {listening ? "Stop" : "Or click here to talk"}
        </Button>
        <Button variant="ghost" disabled={listening || working} onClick={onNext}>Skip for now</Button>
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
