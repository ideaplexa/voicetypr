import { Button } from "@/components/settings/SettingsButton";
import { cn } from "@/lib/utils";
import type { AppSettings } from "@/types";
import { BookA, Bot, Check, Code, FileText, Layers, Send, type LucideIcon } from "lucide-react";
import { useState } from "react";

export type StartingPolishStyle = NonNullable<AppSettings["starting_polish_style"]>;

const USES: { id: string; label: string; icon: LucideIcon; style: StartingPolishStyle; name: string; detail?: string }[] = [
  { id: "messages", label: "Messages and email", icon: Send, style: "Message", name: "Message", detail: "tidy sentences in your own wording" },
  { id: "ai", label: "Prompts for AI", icon: Bot, style: "CleanDictation", name: "Clean" },
  { id: "code", label: "Code and tech notes", icon: Code, style: "Code", name: "Code" },
  { id: "docs", label: "Notes and documents", icon: FileText, style: "Writing", name: "Writing", detail: "for longer text" },
  { id: "clinical", label: "Medical or legal work", icon: BookA, style: "CleanDictation", name: "Clean" },
  { id: "mix", label: "A bit of everything", icon: Layers, style: "CleanDictation", name: "Clean", detail: "the default" },
];

/** The first screen of the deeper path: the style Polish starts in. */
export function UseStep({
  polishOn,
  saving,
  onBack,
  onChoose,
}: {
  /** Polish will run on the next dictation, so the pick applies at once. */
  polishOn: boolean;
  saving: boolean;
  onBack: () => void;
  /** null when the user skips; today's default stays. */
  onChoose: (style: StartingPolishStyle | null) => void;
}) {
  const [picked, setPicked] = useState<string | null>(null);
  const use = USES.find((item) => item.id === picked);
  const style = use ? `the ${use.name} style${use.detail ? `, ${use.detail}` : ""}` : "";

  return (
    <section className="flex w-full flex-col items-center gap-[16px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        What will you mostly dictate?
      </h2>
      <p className="-mt-1 text-center text-base text-muted-foreground">
        Polish starts in a style that fits. You can change it any time.
      </p>
      <div role="group" aria-label="What you mostly dictate" className="grid w-full gap-2.5 sm:grid-cols-3">
        {USES.map((item) => {
          const checked = item.id === picked;
          return (
            <button
              key={item.id}
              type="button"
              aria-pressed={checked}
              onClick={() => setPicked(item.id)}
              className={cn(
                "flex items-center gap-3 rounded-2xl bg-card px-4 py-3.5 text-left text-[15px] font-medium ring-1 ring-foreground/10 transition-colors hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                checked && "ring-[1.5px] ring-sage",
              )}
            >
              <item.icon className={cn("size-[18px] shrink-0", checked ? "text-sage" : "text-muted-foreground")} />
              <span className="flex-1">{item.label}</span>
              <Check className={cn("size-4 shrink-0 text-sage", !checked && "invisible")} />
            </button>
          );
        })}
      </div>
      <p role="status" className="min-h-5 text-center text-sm">
        {use ? polishOn ? `Polish will use ${style}.` : `When Polish is on, it starts in ${style}.` : null}
      </p>
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" disabled={saving} onClick={onBack}>Back</Button>
        <Button variant="ghost" disabled={saving} onClick={() => onChoose(null)}>Skip</Button>
        <Button disabled={saving} onClick={() => onChoose(use?.style ?? null)}>Continue</Button>
      </div>
    </section>
  );
}
