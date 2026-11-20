import { FrameHeading, PrimaryButton } from "@/components/onboarding/Frame";
import { cn } from "@/lib/utils";
import type { AppSettings } from "@/types";
import { Bot, CodeXml, Layers, Mail, NotebookPen, Stethoscope, type LucideIcon } from "lucide-react";
import { useState } from "react";

export type StartingPolishStyle = NonNullable<AppSettings["starting_polish_style"]>;

const USES: { id: string; label: string; icon: LucideIcon; style: StartingPolishStyle }[] = [
  { id: "messages", label: "Messages and email", icon: Mail, style: "Message" },
  { id: "ai", label: "Prompts for AI", icon: Bot, style: "CleanDictation" },
  { id: "code", label: "Code and tech notes", icon: CodeXml, style: "Code" },
  { id: "docs", label: "Notes and documents", icon: NotebookPen, style: "Writing" },
  { id: "clinical", label: "Medical or legal work", icon: Stethoscope, style: "CleanDictation" },
  { id: "mix", label: "A bit of everything", icon: Layers, style: "CleanDictation" },
];

/** 10a What will you mostly dictate: the style Polish starts in, first on the deeper path. */
export function UseStep({
  saving,
  onChoose,
}: {
  saving: boolean;
  /** null without a pick: today's default style stays. */
  onChoose: (style: StartingPolishStyle | null) => void;
}) {
  const [picked, setPicked] = useState<string | null>(null);
  const use = USES.find((item) => item.id === picked);

  return (
    <section className="flex w-[640px] flex-col items-center gap-[34px]">
      <FrameHeading title="What will you mostly dictate?" line="Polish starts in a style that fits. You can change it any time." />
      <div role="group" aria-label="What you mostly dictate" className="grid grid-cols-3 gap-3 self-stretch">
        {USES.map((item) => {
          const checked = item.id === picked;
          return (
            <button
              key={item.id}
              type="button"
              aria-pressed={checked}
              onClick={() => setPicked(item.id)}
              className={cn(
                "flex min-h-[112px] flex-col items-start gap-[18px] rounded-2xl border border-border bg-card p-[18px] text-left hover:border-sage focus-visible:ring-2 focus-visible:ring-sage/40 focus-visible:outline-none",
                checked && "border-[1.5px] border-sage",
              )}
            >
              <span
                className={cn(
                  "flex size-9 items-center justify-center rounded-[9px]",
                  checked ? "bg-sage-bg text-sage" : "bg-secondary text-foreground",
                )}
              >
                <item.icon aria-hidden="true" className="size-[17px]" />
              </span>
              <span className="text-[15px] leading-[1.2] font-semibold text-foreground">{item.label}</span>
            </button>
          );
        })}
      </div>
      <PrimaryButton disabled={saving} onClick={() => onChoose(use?.style ?? null)}>Continue</PrimaryButton>
    </section>
  );
}
