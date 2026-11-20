import { FrameHeading } from "@/components/onboarding/Frame";
import { ArrowRight, CircleCheck, Compass, type LucideIcon } from "lucide-react";

function Choice({ icon: Icon, title, line, onClick }: { icon: LucideIcon; title: string; line: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex min-h-[230px] min-w-0 flex-1 flex-col items-start gap-3.5 rounded-[18px] border border-border bg-card p-[26px] text-left hover:border-sage"
    >
      <span className="flex size-12 shrink-0 items-center justify-center rounded-[13px] bg-sage-bg text-sage">
        <Icon aria-hidden="true" className="size-6" />
      </span>
      <span className="self-stretch text-[19px] leading-[1.25] font-semibold text-foreground">{title}</span>
      <span className="self-stretch text-[15px] leading-[1.45] text-muted-foreground">{line}</span>
      <span className="flex flex-1 items-end self-stretch justify-end text-sage">
        <ArrowRight aria-hidden="true" className="size-5" />
      </span>
    </button>
  );
}

/** 10 That’s the essentials: go deeper now, or finish. */
export function ForkStep({ onDeeper, onDoneForNow }: { onDeeper: () => void; onDoneForNow: () => void }) {
  return (
    <section className="flex w-[760px] flex-col items-center gap-[34px]">
      <FrameHeading title="That’s the essentials" line="Go deeper now, or open this part any time later from Help & feedback." />
      <div className="flex items-stretch gap-4 self-stretch">
        <Choice
          icon={Compass}
          title="Set your style and see what else it does"
          line="Pick what you mostly dictate, then a short tour of seven things Voicetypr can do."
          onClick={onDeeper}
        />
        <Choice icon={CircleCheck} title="Done for now" line="Finish setting up. Everything else waits in Help & feedback." onClick={onDoneForNow} />
      </div>
    </section>
  );
}
