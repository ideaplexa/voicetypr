import { AppMark } from "@/components/onboarding/PromiseStep";
import { FrameHeading, PrimaryButton, QuietButton } from "@/components/onboarding/Frame";
import { cn } from "@/lib/utils";
import { ArrowUpRight, KeyRound, RefreshCw, RotateCcw, WifiOff, type LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

export function IconTile({ icon: Icon, tone }: { icon: LucideIcon; tone: "warn" | "sage" }) {
  return (
    <span className={cn("flex size-16 items-center justify-center rounded-[18px]", tone === "warn" ? "bg-warn-bg text-warn" : "bg-sage-bg text-sage")}>
      <Icon aria-hidden="true" className="size-[30px]" />
    </span>
  );
}

function HaveKey({ onClick }: { onClick: () => void }) {
  return (
    <QuietButton onClick={onClick}>
      <KeyRound aria-hidden="true" className="size-[15px]" />
      I have a key
    </QuietButton>
  );
}

function State({ width, top, title, line, children }: { width: string; top: ReactNode; title: string; line: string; children: ReactNode }) {
  return (
    <section className={cn("flex flex-col items-center gap-[30px]", width)}>
      {top}
      <FrameHeading title={title} line={line} />
      {children}
    </section>
  );
}

/** E0: the licence check failed and nothing cached says licensed or trial. Never "trial ended". */
export function CheckFailedStep({ checking, onRecheck }: { checking: boolean; onRecheck: () => void }) {
  return (
    <State
      width="w-[480px]"
      top={<IconTile icon={RefreshCw} tone="warn" />}
      title="Couldn’t check your licence"
      line="Voicetypr couldn’t reach its licence server. This doesn’t mean your trial has ended."
    >
      <PrimaryButton disabled={checking} onClick={onRecheck}>
        <RotateCcw aria-hidden="true" className="size-4" />
        Recheck
      </PrimaryButton>
    </State>
  );
}

/** E2: the trial has ended, confirmed by the licence server. */
export function TrialEndedStep({ onBuy, onHaveKey }: { onBuy: () => void; onHaveKey: () => void }) {
  return (
    <State
      width="w-[500px]"
      top={<AppMark className="size-[60px]" />}
      title="Your free week is over"
      line="Dictation is paused. Your history, Dictionary and settings are all still here."
    >
      <PrimaryButton className="w-[280px]" onClick={onBuy}>
        Buy now
        <ArrowUpRight aria-hidden="true" className="size-[15px]" />
      </PrimaryButton>
      <HaveKey onClick={onHaveKey} />
    </State>
  );
}

/** E1: Start trial couldn't reach Voicetypr, so no trial started. */
export function TrialUnreachableStep({ pending, onTryAgain, onHaveKey }: { pending: boolean; onTryAgain: () => void; onHaveKey: () => void }) {
  return (
    <State
      width="w-[460px]"
      top={<IconTile icon={WifiOff} tone="warn" />}
      title="Couldn’t reach Voicetypr"
      line="We couldn’t confirm your trial. Check your connection and try again."
    >
      <PrimaryButton disabled={pending} onClick={onTryAgain}>
        <RotateCcw aria-hidden="true" className="size-4" />
        Try again
      </PrimaryButton>
      <HaveKey onClick={onHaveKey} />
    </State>
  );
}
