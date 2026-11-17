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
      {copyOnly ? (
        <p className="max-w-[480px] text-sm text-muted-foreground">
          Your words are copied: press {isMacOS ? "⌘V" : "Ctrl+V"} to paste them. Turn on Accessibility in
          System Settings and it pastes for you.
        </p>
      ) : null}
      {consent}
      <Button size="lg" disabled={saving} onClick={onFinish}>
        {saving ? <Spinner /> : null}Start using Voicetypr
      </Button>
    </section>
  );
}
