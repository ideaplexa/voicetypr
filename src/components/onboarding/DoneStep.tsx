import { KeyCaps } from "@/components/KeyCaps";
import { AppMark } from "@/components/onboarding/PromiseStep";
import { Band } from "@/components/onboarding/OnboardingChrome";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { PanelTop } from "lucide-react";
import { Button } from "@/components/settings/SettingsButton";
import { Spinner } from "@/components/ui/spinner";
import { isMacOS } from "@/lib/platform";
import type { ReactNode } from "react";

const DOTS: [number, number, boolean][] = [
  [-230, -30, true], [-170, 30, false], [-120, -42, false], [-60, 46, true], [80, -46, false], [140, 34, true], [200, -20, false], [250, 40, false],
];

export function DoneStep({
  title,
  detail,
  caps,
  holdToTalk,
  copyOnly,
  consent,
  saving,
  onBack,
  onFinish,
}: {
  title: string;
  detail: string | null;
  caps: string[];
  holdToTalk: boolean;
  /** Accessibility is off, so Voicetypr copies instead of pasting. */
  copyOnly: boolean;
  consent: ReactNode;
  saving: boolean;
  onBack: () => void;
  onFinish: () => void;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-[20px] text-center">
      <Band>
        {DOTS.map(([x, y, gold]) => (
          <span key={`${x},${y}`} className={cn("absolute size-2.5 rounded-full", gold ? "bg-[#B08D57]/70" : "bg-sage/50")} style={{ left: x, top: y }} />
        ))}
        <AppMark />
      </Band>
      <h2 className="text-[28px] leading-[normal] font-semibold tracking-[-0.5px]">{title}</h2>
      <p className="max-w-[520px] text-base leading-9 text-muted-foreground">
        {detail ? <>{detail} </> : null}
        {holdToTalk ? "Hold" : "Press"} <KeyCaps caps={caps} size="onboarding" /> in any app and talk.
      </p>
      <Card className="w-full flex-row items-start gap-3 px-4 py-3.5 text-left">
        <span className="flex size-[34px] shrink-0 items-center justify-center rounded-[9px] bg-sage-bg">
          <PanelTop className="size-[18px] text-sage" />
        </span>
        <p className="text-sm leading-6">
          <span className="font-semibold">Voicetypr stays in your {isMacOS ? "menu bar" : "system tray"}</span> when this
          window closes. Its menu starts dictation, switches Polish, the engine and the microphone, and opens Settings.
        </p>
      </Card>
      {copyOnly ? (
        <p className="max-w-[480px] text-sm text-muted-foreground">
          Your words are copied: press {isMacOS ? "⌘V" : "Ctrl+V"} to paste them. Turn on Accessibility in
          System Settings and it pastes for you.
        </p>
      ) : null}
      {consent}
      <div className="flex flex-wrap items-center justify-center gap-[10px]">
        <Button size="lg" variant="outline" disabled={saving} onClick={onBack}>Back</Button>
        <Button size="lg" disabled={saving} onClick={onFinish}>
          {saving ? <Spinner /> : null}Start using Voicetypr
        </Button>
      </div>
    </section>
  );
}
