import appIcon from "../../../src-tauri/icons/128x128@2x.png";
import { PillPreview } from "@/components/discovery/PillPreview";
import { FrameHeading, FrameKeys, PrimaryButton, QuietButton, SecondaryButton } from "@/components/onboarding/Frame";
import type { PermissionStatus } from "@/components/onboarding/onboardingTypes";
import { useFirstDictation, type FirstDictation } from "@/components/onboarding/useFirstDictation";
import { isMacOS } from "@/lib/platform";
import { MicOff, PencilLine } from "lucide-react";
import { useState } from "react";

/** A local model still downloading, for 08b. */
export interface TryDownload {
  model: string;
  progress: number | undefined;
  /** Bytes, when known. */
  size: number | undefined;
  error: string | undefined;
  onRetry: () => void;
}

const mb = (bytes: number) => Math.round(bytes / (1024 * 1024));

/** 08 Try it for real, and 08b Almost ready while the model downloads. */
export function TryStep({
  caps,
  holdToTalk,
  engineReady,
  download,
  microphone,
  clickToTalk,
  onAllowMicrophone,
  onResult,
  onSkip,
}: {
  caps: string[];
  holdToTalk: boolean;
  engineReady: boolean;
  /** Set while the on-device model is not ready. */
  download: TryDownload | null;
  microphone: PermissionStatus;
  /** macOS without Accessibility: the shortcut can't reach the app, so the note offers a click. */
  clickToTalk: boolean;
  onAllowMicrophone: () => Promise<boolean>;
  onResult: (result: FirstDictation) => void;
  /** Moves on without a dictation of their own. */
  onSkip: () => void;
}) {
  const [waited, setWaited] = useState(!engineReady);
  if (!engineReady && !waited) setWaited(true);
  const { listening, working, allowance, start, stop } = useFirstDictation(engineReady && !waited, holdToTalk, onResult);
  const micOff = microphone === "denied" || microphone === "restricted";

  if (waited) {
    const percent = download?.progress === undefined ? (engineReady ? 100 : 0) : Math.round(download.progress);
    return (
      <section className="flex w-[560px] flex-col items-center gap-[30px]">
        <FrameHeading
          title="Almost ready"
          line={download
            ? `${download.model} is still downloading. You can try it the moment it’s done.`
            : "Your engine isn’t connected yet. Go back to choose it again."}
        />
        {download ? (
          <div className="flex flex-col gap-3 self-stretch rounded-2xl border border-border bg-card p-6">
            <div className="flex items-center">
              <span className="flex-1 text-base font-semibold text-foreground">{download.model}</span>
              <span className="text-sm text-muted-foreground tabular-nums">
                {download.size ? `${mb((download.size * percent) / 100)} of ${mb(download.size)} MB` : `${percent}%`}
              </span>
            </div>
            <span
              role="progressbar"
              aria-label={`Downloading ${download.model}`}
              aria-valuenow={percent}
              aria-valuemin={0}
              aria-valuemax={100}
              className="relative h-1.5 self-stretch overflow-hidden rounded-[3px] bg-border"
            >
              <span className="absolute inset-y-0 left-0 rounded-[3px] bg-sage" style={{ width: `${percent}%` }} />
            </span>
            {download.error ? (
              <p role="alert" className="flex items-center justify-between gap-3 text-sm text-destructive">
                The download stopped.
                <SecondaryButton onClick={download.onRetry}>Retry</SecondaryButton>
              </p>
            ) : null}
          </div>
        ) : null}
        <PrimaryButton disabled={!engineReady} onClick={() => setWaited(false)}>Try it</PrimaryButton>
      </section>
    );
  }

  if (allowance?.kind === "usedUp") {
    return (
      <section className="flex w-[560px] flex-col items-center gap-[30px]">
        <FrameHeading title="Your first tries are used up" line={`This ${isMacOS ? "Mac" : "PC"} has had its first tries.`} />
        <PrimaryButton onClick={onSkip}>Continue</PrimaryButton>
      </section>
    );
  }

  const done = holdToTalk ? "Let go when you’re done." : "Press it again when you’re done.";
  return (
    <section className="flex w-[600px] flex-col items-center gap-[30px]">
      <FrameHeading
        title="Try it for real"
        line={`Click into the note, ${holdToTalk ? "hold" : "press"} your shortcut and say anything. Messy is fine. ${done}`}
      />
      <FrameKeys caps={caps} />
      {micOff ? (
        <div role="alert" className="flex items-center gap-3 self-stretch rounded-[14px] bg-warn-bg px-4 py-3 text-left text-sm text-warn">
          <MicOff aria-hidden="true" className="size-5 shrink-0" />
          <p className="flex-1">The microphone is off for Voicetypr, so it can’t hear you yet. Switch it on, then come straight back.</p>
          <SecondaryButton onClick={() => void onAllowMicrophone()}>
            {isMacOS ? "Open System Settings" : "Open Privacy settings"}
          </SecondaryButton>
        </div>
      ) : null}
      <div
        tabIndex={0}
        aria-label="Try it here"
        className="group flex h-[150px] flex-col self-stretch overflow-hidden rounded-[14px] border border-border bg-card outline-none focus-visible:ring-2 focus-visible:ring-sage/40"
      >
        <div className="flex h-[34px] shrink-0 items-center gap-2 bg-secondary px-3.5">
          <PencilLine aria-hidden="true" className="size-[13px] text-muted-foreground" />
          <span className="text-[13px] font-medium text-muted-foreground">Try it here</span>
        </div>
        <div className="flex flex-1 flex-col items-start gap-1.5 px-[18px] py-4">
          <p role="status" aria-live="polite" className="text-[15px] text-muted-foreground">
            {working ? "Transcribing…" : listening ? "Listening…" : <span aria-hidden="true" className="block h-5 w-0.5 bg-sage" />}
          </p>
        </div>
      </div>
      {clickToTalk ? (
        holdToTalk ? (
          <QuietButton
            onPointerDown={() => void start()}
            onPointerUp={() => void stop()}
            onPointerLeave={() => { if (listening) void stop(); }}
          >
            Or press and hold here
          </QuietButton>
        ) : (
          <QuietButton onClick={() => void (listening ? stop() : start())}>Or click here to talk</QuietButton>
        )
      ) : null}
      <PillPreview appIcon={appIcon} state={working ? "transcribing" : "listening"} scale={1.3} />
    </section>
  );
}
