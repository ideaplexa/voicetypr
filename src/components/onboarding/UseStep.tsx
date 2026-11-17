import { Button } from "@/components/settings/SettingsButton";
import { cn } from "@/lib/utils";
import type { AppSettings } from "@/types";
import { Check } from "lucide-react";
import { useState } from "react";

export type StartingPolishStyle = NonNullable<AppSettings["starting_polish_style"]>;

const USES: { id: string; label: string; style: StartingPolishStyle; name: string; detail?: string }[] = [
  { id: "messages", label: "Messages and email", style: "Message", name: "Message", detail: "tidy sentences in your own wording" },
  { id: "ai", label: "Prompts for AI", style: "CleanDictation", name: "Clean" },
  { id: "code", label: "Code and tech notes", style: "Code", name: "Code" },
  { id: "docs", label: "Notes and documents", style: "Writing", name: "Writing", detail: "for longer text" },
  { id: "clinical", label: "Medical or legal work", style: "CleanDictation", name: "Clean" },
  { id: "mix", label: "A bit of everything", style: "CleanDictation", name: "Clean", detail: "the default" },
];

/** "What will you mostly dictate?": picks the style Polish starts in. */
export function UseStep({
  polishAvailable,
  saving,
  onBack,
  onChoose,
}: {
  polishAvailable: boolean;
  saving: boolean;
  onBack: () => void;
  /** null when the user skips; today's default stays. */
  onChoose: (style: StartingPolishStyle | null) => void;
}) {
  const [picked, setPicked] = useState<string | null>(null);
  const use = USES.find((item) => item.id === picked);
  const style = use ? `the ${use.name} style${use.detail ? `, ${use.detail}` : ""}` : "";

  return (
    <section className="flex w-full flex-col items-center gap-[18px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        What will you mostly dictate?
      </h2>
      <p className="text-center text-sm text-muted-foreground">
        Optional. It sets the starting style for Polish. You can change it any time in Settings.
      </p>
      <div role="group" aria-label="What you mostly dictate" className="grid w-full gap-2.5 sm:grid-cols-2">
        {USES.map((item) => {
          const checked = item.id === picked;
          return (
            <button
              key={item.id}
              type="button"
              aria-pressed={checked}
              onClick={() => setPicked(item.id)}
              className={cn(
                "flex items-center justify-between rounded-2xl bg-card px-4 py-3.5 text-left text-sm font-medium ring-1 ring-foreground/10 transition-colors hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                checked && "ring-[1.5px] ring-sage",
              )}
            >
              {item.label}
              <Check className={cn("size-4 text-sage", !checked && "invisible")} />
            </button>
          );
        })}
      </div>
      <p role="status" className="min-h-5 text-center text-[13px] text-muted-foreground">
        {use
          ? polishAvailable
            ? `Polish will use ${style}.`
            : `When you turn on Polish, it starts in ${style}.`
          : null}
      </p>
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" disabled={saving} onClick={onBack}>Back</Button>
        <Button variant="ghost" disabled={saving} onClick={() => onChoose(null)}>Skip</Button>
        <Button disabled={!use || saving} onClick={() => use && onChoose(use.style)}>Continue</Button>
      </div>
    </section>
  );
}
