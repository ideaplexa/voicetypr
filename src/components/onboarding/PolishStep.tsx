import { Button } from "@/components/settings/SettingsButton";
import { Switch } from "@/components/settings/SettingsSwitch";
import { builtinPolishView } from "@/components/polish/BuiltInPolishSetting";
import { Band, BandWord } from "@/components/onboarding/OnboardingChrome";
import { AppMark } from "@/components/onboarding/PromiseStep";
import { Card } from "@/components/ui/card";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import type { AppSettings, PolishStatus } from "@/types";
import { BookA, Bot, Check, Code, FileText, Layers, Send, Sparkles, type LucideIcon } from "lucide-react";
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

const EXAMPLE = {
  heard: "um, so let’s move the sync to Thursday, no wait, Friday at three, and I’ll send the notes before",
  typed: "Let’s move the sync to Friday at 3, and I’ll send the notes before.",
};

/** What Polish is, whether it can run here, and the style it starts in. */
export function PolishStep({
  status,
  builtinChosen,
  onBuiltinChange,
  saving,
  onBack,
  onChoose,
}: {
  /** `polish_status`, the gate dictation uses; null while it is being asked. */
  status: PolishStatus | null;
  /** The saved built-in Polish choice, shared with Settings → Polish. */
  builtinChosen: boolean;
  onBuiltinChange: (on: boolean) => void;
  saving: boolean;
  onBack: () => void;
  /** null when the user skips; today's default stays. */
  onChoose: (style: StartingPolishStyle | null) => void;
}) {
  const [picked, setPicked] = useState<string | null>(null);
  const use = USES.find((item) => item.id === picked);
  const style = use ? `the ${use.name} style${use.detail ? `, ${use.detail}` : ""}` : "";
  const ownReady = status?.own_provider_ready === true;
  const builtin = builtinPolishView(ownReady ? "own_provider" : status?.builtin ?? null, builtinChosen);
  const on = ownReady ? status?.own_provider_enabled === true : builtin.checked;

  return (
    <section className="flex w-full flex-col items-center gap-[16px]">
      <Band>
        <BandWord struck className="-left-[210px] top-5">um</BandWord>
        <BandWord className="-left-[150px] -top-6">Messages</BandWord>
        <BandWord className="-left-[100px] top-6">Prompts</BandWord>
        <AppMark />
        <BandWord struck className="left-[70px] -top-6">no wait</BandWord>
        <BandWord className="left-[100px] top-6">Code</BandWord>
        <BandWord className="left-[170px] -top-5">Notes</BandWord>
      </Band>
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        What will you mostly dictate?
      </h2>
      <p className="-mt-1 text-center text-base text-muted-foreground">
        Optional. It picks a starting point you can change any time.
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
        {use ? on ? `Polish will use ${style}.` : `When Polish is on, it starts in ${style}.` : null}
      </p>
      <Card className="w-full gap-3 px-[18px] py-4 text-left">
        {ownReady ? (
          <div className="flex items-start gap-3">
            <Sparkles className={cn("mt-0.5 size-4 shrink-0", on ? "text-sage" : "text-muted-foreground")} />
            <p className="text-sm">
              {on ? (
                <><span className="font-semibold">Polish is on</span> and set up with your AI provider.</>
              ) : (
                <><span className="font-semibold">Polish is set up but off.</span> Turn it on in Settings → Polish, or from Voicetypr’s {isMacOS ? "menu bar" : "tray"} menu.</>
              )}
            </p>
          </div>
        ) : (
          <div className="flex items-start gap-3">
            <Sparkles className={cn("mt-0.5 size-4 shrink-0", on ? "text-sage" : "text-muted-foreground")} />
            <div className="flex-1 text-sm">
              <label htmlFor="onboarding-builtin-polish" className="font-semibold">Clean up what I say</label>
              <p className="text-muted-foreground">
                Your words go to Voicetypr’s server to be cleaned, and nothing is kept. Off sends nothing; you get
                exactly what you said.
              </p>
              {builtin.checked ? null : <p className="mt-1">{builtin.status}</p>}
            </div>
            <Switch id="onboarding-builtin-polish" checked={builtin.checked} disabled={builtin.disabled || saving}
              onCheckedChange={onBuiltinChange} />
          </div>
        )}
        <div className="grid gap-2 rounded-xl bg-sage-bg px-4 py-3 text-[15px] leading-[22px] sm:grid-cols-[auto_1fr] sm:gap-x-4">
          <span className="text-sm font-medium text-muted-foreground">You say</span>
          <p className="text-muted-foreground">{EXAMPLE.heard}</p>
          <span className="text-sm font-medium text-sage">With Polish</span>
          <p>{EXAMPLE.typed}</p>
        </div>
        {ownReady ? null : (
          <p className="pl-7 text-sm text-muted-foreground">Rather use your own AI provider? Add its key in Settings → Polish.</p>
        )}
      </Card>
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" disabled={saving} onClick={onBack}>Back</Button>
        <Button variant="ghost" disabled={saving} onClick={() => onChoose(null)}>Skip</Button>
        <Button disabled={saving} onClick={() => onChoose(use?.style ?? null)}>Continue</Button>
      </div>
    </section>
  );
}
