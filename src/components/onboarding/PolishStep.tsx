import { Button } from "@/components/settings/SettingsButton";
import { Card } from "@/components/ui/card";
import type { FirstDictation } from "@/components/onboarding/useFirstDictation";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import type { AppSettings } from "@/types";
import { Check, Sparkles } from "lucide-react";
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

const EXAMPLE = {
  heard: "um, so let’s move the sync to Thursday, no wait, Friday at three, and I’ll send the notes before",
  typed: "Let’s move the sync to Friday at 3, and I’ll send the notes before.",
};

/** What Polish is, whether it can run here, and the style it starts in. */
export function PolishStep({
  canRun,
  enabled,
  firstTry,
  saving,
  onBack,
  onChoose,
}: {
  /** The app's own gate: an AI provider with a model and key is set up. */
  canRun: boolean;
  enabled: boolean;
  /** The user's first dictation, when Polish cleaned it. */
  firstTry: FirstDictation | null;
  saving: boolean;
  onBack: () => void;
  /** null when the user skips; today's default stays. */
  onChoose: (style: StartingPolishStyle | null) => void;
}) {
  const [picked, setPicked] = useState<string | null>(null);
  const use = USES.find((item) => item.id === picked);
  const style = use ? `the ${use.name} style${use.detail ? `, ${use.detail}` : ""}` : "";
  const own = firstTry?.polished ? firstTry : null;
  const pair = own ?? EXAMPLE;
  const on = canRun && enabled;

  return (
    <section className="flex w-full flex-col items-center gap-[18px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        Polish tidies what you say
      </h2>
      <p className="max-w-[520px] text-center text-sm text-muted-foreground">
        It removes filler words and false starts, follows your corrections and fixes punctuation, in the style
        you choose.
      </p>
      <div className="grid w-full gap-3 sm:grid-cols-2">
        <Card className="gap-2 px-[18px] py-4">
          <span className="text-[11.5px] font-medium tracking-wide text-muted-foreground uppercase">
            {own ? "You said" : "Example · you say"}
          </span>
          <p className="text-[15px] leading-[22px] text-muted-foreground">{pair.heard}</p>
        </Card>
        <Card className="gap-2 px-[18px] py-4 outline-[1.5px] outline-sage -outline-offset-1">
          <span className="text-[11.5px] font-medium tracking-wide text-muted-foreground uppercase">
            {own ? "Polish typed" : "With Polish"}
          </span>
          <p className="text-[15px] leading-[22px]">{pair.typed}</p>
        </Card>
      </div>
      {/* VOIC-14: the built-in provider's on/off switch and its "your text goes to Voicetypr's server" line go in this card. */}
      <Card className="w-full flex-row items-start gap-3 px-4 py-3 text-left">
        <Sparkles className={cn("mt-0.5 size-4 shrink-0", on ? "text-sage" : "text-muted-foreground")} />
        <p className="text-sm">
          {on ? (
            <><span className="font-semibold">Polish is on</span> and set up with your AI provider.</>
          ) : canRun ? (
            <><span className="font-semibold">Polish is set up but off.</span> Turn it on in Settings → Polish, or from Voicetypr’s {isMacOS ? "menu bar" : "tray"} menu.</>
          ) : (
            <>
              <span className="font-semibold">Polish isn’t on yet.</span> It needs an AI provider. After setup, open
              Settings → Polish and add your key or connect a provider; until then you get exactly what you said.
            </>
          )}
        </p>
      </Card>
      <h3 className="mt-1 text-center text-base font-semibold">What will you mostly dictate?</h3>
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
                "flex items-center justify-between rounded-2xl bg-card px-4 py-3 text-left text-[13px] font-medium ring-1 ring-foreground/10 transition-colors hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                checked && "ring-[1.5px] ring-sage",
              )}
            >
              {item.label}
              <Check className={cn("size-4 shrink-0 text-sage", !checked && "invisible")} />
            </button>
          );
        })}
      </div>
      <p role="status" className="min-h-5 text-center text-[13px] text-muted-foreground">
        {use
          ? on ? `Polish will use ${style}.` : `When Polish is on, it starts in ${style}.`
          : "Optional. It sets the style Polish starts in; you can change it any time in Settings."}
      </p>
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" disabled={saving} onClick={onBack}>Back</Button>
        <Button variant="ghost" disabled={saving} onClick={() => onChoose(null)}>Skip</Button>
        <Button disabled={!use || saving} onClick={() => use && onChoose(use.style)}>Continue</Button>
      </div>
    </section>
  );
}
