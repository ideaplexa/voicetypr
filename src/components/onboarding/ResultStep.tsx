import { Button } from "@/components/settings/SettingsButton";
import { Card } from "@/components/ui/card";
import { MESSY, PLAIN } from "@/components/onboarding/TryStep";
import type { FirstDictation } from "@/components/onboarding/useFirstDictation";
import { cn } from "@/lib/utils";
import { CircleCheck } from "lucide-react";
import type { ReactNode } from "react";

/**
 * The First try's result. When built-in Polish cleaned the try, both sides are
 * the user's own words; otherwise the cleaned side is the labelled example,
 * never presented as theirs.
 */
export function ResultStep({
  result,
  polishAvailable,
  onTryAgain,
  onBack,
  onNext,
}: {
  /** Null when they skipped the try or had no tries left. */
  result: FirstDictation | null;
  polishAvailable: boolean;
  onTryAgain: () => void;
  onBack: () => void;
  onNext: () => void;
}) {
  if (result?.polished) {
    return (
      <section className="flex w-full flex-col items-center gap-[18px] text-center">
        <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">That’s you, cleaned up</h2>
        <div className="grid w-full gap-3 text-left sm:grid-cols-2">
          <Pane label="You said">{result.heard}</Pane>
          <Pane label="Voicetypr cleaned it" highlight>{result.typed}</Pane>
        </div>
        <p role="status" className="flex items-center gap-2 text-[12.5px] font-medium text-muted-foreground">
          <CircleCheck className="size-4 text-sage" />
          Worked · {result.words} {result.words === 1 ? "word" : "words"}
        </p>
        <div className="flex flex-wrap items-center justify-center gap-[10px]">
          <Button variant="ghost" onClick={onTryAgain}>Try again</Button>
          <Button onClick={onNext}>Continue</Button>
        </div>
      </section>
    );
  }
  return (
    <section className="flex w-full flex-col items-center gap-[18px] text-center">
      <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        {result ? "That’s your voice, as text" : "Here’s what a dictation looks like"}
      </h2>
      <div className={cn("grid w-full gap-3 text-left", polishAvailable && "sm:grid-cols-2")}>
        <Pane label={result ? "What Voicetypr heard" : "Example: what you said"} highlight={Boolean(result)}>
          {result ? result.heard : polishAvailable ? MESSY : PLAIN}
        </Pane>
        {polishAvailable ? (
          <Pane label="Example: what Polish makes of a messy sentence">{PLAIN}</Pane>
        ) : null}
      </div>
      {result ? (
        <p role="status" className="flex items-center gap-2 text-[12.5px] font-medium text-muted-foreground">
          <CircleCheck className="size-4 text-sage" />
          Worked · {result.words} {result.words === 1 ? "word" : "words"} · exactly what the engine heard
        </p>
      ) : null}
      {polishAvailable ? (
        <p className="max-w-[480px] text-[12.5px] text-muted-foreground">
          Polish didn’t run on this try. The cleaned text is an example.
        </p>
      ) : null}
      <div className="flex flex-wrap items-center justify-center gap-[10px]">
        <Button variant="ghost" onClick={result ? onTryAgain : onBack}>{result ? "Try again" : "Back"}</Button>
        <Button onClick={onNext}>Continue</Button>
      </div>
    </section>
  );
}

function Pane({ label, highlight, children }: { label: string; highlight?: boolean; children: ReactNode }) {
  return (
    <Card className={cn("min-h-[120px] gap-2 px-[18px] py-4", highlight && "outline-[1.5px] outline-sage -outline-offset-1")}>
      <span className="text-[11.5px] font-medium tracking-wide text-muted-foreground uppercase">{label}</span>
      <p className="text-[16px] leading-[24px] text-foreground">{children}</p>
    </Card>
  );
}
