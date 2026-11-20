import { PrimaryButton, QuietButton, SecondaryButton, Tag, machine } from "@/components/onboarding/Frame";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { EXAMPLE, ProofPair } from "@/components/onboarding/Proof";
import type { FirstDictation } from "@/components/onboarding/useFirstDictation";
import { AppWindow, ArrowUpRight, BadgeCheck, Cloud, KeyRound, Lock, Network, Sparkles, type LucideIcon } from "lucide-react";

/** 11 Buy Voicetypr (11b cloud engine, 11c another computer): the proof, four outcomes, Buy now, then the trial. */
export function BuyStep({
  result,
  source,
  pending,
  onBuy,
  onStartTrial,
  onHaveKey,
}: {
  /** Their first try, or null when it was skipped or used up. */
  result: FirstDictation | null;
  source: SourceType;
  pending: boolean;
  onBuy: () => void;
  /** Goes to the name and email page. */
  onStartTrial: () => void;
  onHaveKey: () => void;
}) {
  const ownWords = result !== null && !result.polished;
  const outcomes: [LucideIcon, string][] = [
    [AppWindow, "Works in every app you type in"],
    [Sparkles, ownWords ? "Turn on Polish: messy speech lands clean" : "Messy speech lands clean, no AI key"],
    source === "cloud" ? [Cloud, "Your chosen cloud engine, your own key"]
      : source === "remote" ? [Network, "Runs on your other computer’s engine"]
      : [Lock, `Private: runs on your ${machine}, works offline`],
    [BadgeCheck, "One payment, yours to keep"],
  ];
  return (
    <section className="flex w-[780px] flex-col items-center gap-[30px]">
      <h2 className="text-center text-[36px] leading-[1.2] font-semibold tracking-[-0.9px] text-foreground">
        Keep talking like that, everywhere
      </h2>
      {result?.polished ? (
        <ProofPair raw={result.heard} clean={result.typed} size="lg" />
      ) : ownWords ? (
        <div className="flex flex-col items-start gap-3 self-stretch rounded-2xl border-[1.5px] border-sage bg-card px-[22px] py-5">
          <span className="text-sm leading-[1.2] font-semibold text-sage">You said it. Voicetypr typed it.</span>
          <p className="self-stretch text-lg leading-[1.45] font-medium whitespace-pre-wrap text-foreground">{result.heard}</p>
        </div>
      ) : (
        <div className="flex flex-col items-center gap-3 self-stretch">
          <Tag tone="plain">Example of Polish</Tag>
          <ProofPair raw={EXAMPLE.raw} dropped={EXAMPLE.dropped} clean={EXAMPLE.clean} rawLabel="You say" cleanLabel="It lands" size="sm" arrow />
        </div>
      )}
      <ul className="grid grid-cols-2 gap-x-6 gap-y-4 self-stretch px-6">
        {outcomes.map(([Icon, text]) => (
          <li key={text} className="flex items-center gap-3 text-[17px] font-medium text-foreground">
            <Icon aria-hidden="true" className="size-[22px] shrink-0 text-sage" />
            {text}
          </li>
        ))}
      </ul>
      <div className="flex flex-col items-center gap-3">
        <PrimaryButton size="lg" className="w-[340px]" disabled={pending} onClick={onBuy}>
          Buy now
          <ArrowUpRight aria-hidden="true" className="size-[17px]" />
        </PrimaryButton>
        <SecondaryButton className="w-[340px]" disabled={pending} onClick={onStartTrial}>Start 7-day trial · No card</SecondaryButton>
        <div className="flex items-center gap-4">
          <QuietButton disabled={pending} onClick={onHaveKey}>
            <KeyRound aria-hidden="true" className="size-[15px]" />
            I have a key
          </QuietButton>
          <p className="text-sm text-muted-foreground">Made by Moinul. I read every email.</p>
        </div>
      </div>
    </section>
  );
}
