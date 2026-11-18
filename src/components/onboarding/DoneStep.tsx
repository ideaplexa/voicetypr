import { KeyCaps } from "@/components/KeyCaps";
import { AppMark } from "@/components/onboarding/PromiseStep";
import { Button } from "@/components/settings/SettingsButton";
import { Spinner } from "@/components/ui/spinner";
import { isMacOS } from "@/lib/platform";
import type { ReactNode } from "react";

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
      <AppMark />
      <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">{title}</h2>
      <p className="max-w-[520px] text-base leading-9 text-muted-foreground">
        {detail ? <>{detail} </> : null}
        {holdToTalk ? "Hold" : "Press"} <KeyCaps caps={caps} size="onboarding" /> in any app and talk.
      </p>
      <p className="max-w-[500px] text-sm text-muted-foreground">
        {isMacOS
          ? "When this window closes, Voicetypr keeps running in your menu bar. Its menu starts dictation, switches Polish, the engine and the microphone, and opens Settings."
          : "When this window closes, Voicetypr keeps running in the system tray. Its menu starts dictation, switches Polish, the engine and the microphone, and opens Settings."}
      </p>
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
