import appIcon from "../../../src-tauri/icons/128x128@2x.png";
import { PillPreview } from "@/components/discovery/PillPreview";
import { FrameHeading, PrimaryButton, QuietButton } from "@/components/onboarding/Frame";
import { ProofPair } from "@/components/onboarding/Proof";
import type { FirstDictation } from "@/components/onboarding/useFirstDictation";
import { RotateCcw } from "lucide-react";

/** 09 That’s you, cleaned up; 09b word for word when Polish didn’t run. */
export function ResultStep({
  result,
  polishOn,
  onTryAgain,
  onNext,
}: {
  result: FirstDictation;
  /** Polish is switched on, though it may not have run on this try. */
  polishOn: boolean;
  onTryAgain: () => void;
  onNext: () => void;
}) {
  return (
    <section className="flex w-[720px] flex-col items-center gap-7">
      {result.polished ? (
        <FrameHeading title="That’s you, cleaned up" line="Your words as you said them, and what Voicetypr made of them." />
      ) : (
        <FrameHeading
          title="That’s you, word for word"
          line={polishOn
            ? "Polish didn’t run on this try, so there was no AI clean-up. Here is what landed in your note."
            : "Polish is off, so there was no AI clean-up. Here is what landed in your note."}
        />
      )}
      <PillPreview appIcon={appIcon} wordCount={result.words} state="pasted" scale={1.2} />
      {result.polished ? (
        <ProofPair raw={result.heard} clean={result.typed} />
      ) : (
        <div className="flex flex-col items-start gap-3 self-stretch rounded-[14px] bg-secondary px-[22px] py-5">
          <span className="text-[13.5px] leading-[1.2] font-semibold text-muted-foreground">You said</span>
          <p className="self-stretch text-base leading-[1.5] whitespace-pre-wrap text-foreground">{result.heard}</p>
        </div>
      )}
      <div className="flex items-center gap-3.5">
        <QuietButton onClick={onTryAgain}>
          <RotateCcw aria-hidden="true" className="size-[15px]" />
          Try again
        </QuietButton>
        <PrimaryButton onClick={onNext}>Continue</PrimaryButton>
      </div>
    </section>
  );
}
