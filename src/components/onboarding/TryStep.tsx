import { KeyCaps } from "@/components/KeyCaps";
import { Button } from "@/components/settings/SettingsButton";
import { Card } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import type { PermissionStatus } from "@/components/onboarding/onboardingTypes";
import { useFirstDictation } from "@/components/onboarding/useFirstDictation";
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
  preparing,
  microphone,
  onAllowMicrophone,
  onBack,
  onNext,
  onWorked,
}: {
  caps: string[];
  holdToTalk: boolean;
  polishAvailable: boolean;
  engineReady: boolean;
  /** Download progress for the engine while it is not ready. */
  preparing: ReactNode;
  microphone: PermissionStatus;
  onAllowMicrophone: () => Promise<boolean>;
  onBack: () => void;
  onNext: () => void;
  onWorked: () => void;
}) {
  const { listening, working, result, busy, talk, reset } = useFirstDictation(engineReady);
  useEffect(() => {
    if (result) onWorked();
  }, [result, onWorked]);
  const micOff = microphone === "denied" || microphone === "restricted";
  // Both panes only when Polish could change the text and, once heard, did.
  const sideBySide = result ? result.polished : polishAvailable;
  const verb = holdToTalk ? "Hold" : "Press";

  if (!engineReady) {
    return (
      <section className="flex w-full flex-col items-center gap-[18px] text-center">
        <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">Almost ready</h2>
        <p className="max-w-[480px] text-sm leading-8 text-muted-foreground">
          The dictation engine downloads once. Finish downloading, then {verb.toLowerCase()}{" "}
          <KeyCaps caps={caps} size="onboarding" /> and say your sentence.
        </p>
        {preparing}
        <Button variant="outline" onClick={onBack}>Back</Button>
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
            <Button variant="ghost" onClick={onNext}>Skip for now</Button>
          </>
        )}
      </div>
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
