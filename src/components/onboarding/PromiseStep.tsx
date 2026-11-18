import appIcon from "../../../src-tauri/icons/128x128@2x.png";
import { Button } from "@/components/settings/SettingsButton";
import { Card } from "@/components/ui/card";
import { ChevronRight } from "lucide-react";

export function AppMark() {
  return <img src={appIcon} alt="" aria-hidden="true" className="size-14 rounded-[14px]" />;
}

export function PromiseStep({
  polishAvailable,
  nextLine,
  busy,
  onBegin,
}: {
  polishAvailable: boolean;
  /** What comes after this screen, such as starting the free week. */
  nextLine: string;
  busy: boolean;
  onBegin: () => void;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-[20px] text-center">
      <AppMark />
      <h1 className="text-[40px] leading-[1.08] font-semibold tracking-[-1.2px] text-balance">
        {polishAvailable ? <>Say it messy.<br />It lands clean.</> : <>Say it.<br />It lands in any app.</>}
      </h1>
      <Card aria-hidden="true" className="w-full max-w-[520px] gap-2 px-5 py-4 text-left">
        <span className="text-[11.5px] font-medium tracking-wide text-muted-foreground uppercase">Example</span>
        {polishAvailable ? (
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2">
            <dt className="text-[13px] leading-6 text-muted-foreground">You say</dt>
            <dd className="text-[15px] leading-6 text-muted-foreground">um, so let’s move the sync to Thursday, no wait, Friday at three</dd>
            <dt className="text-[13px] leading-6 text-muted-foreground">It lands</dt>
            <dd className="text-[16px] leading-6">Let’s move the sync to Friday at 3.</dd>
          </dl>
        ) : (
          <p className="text-[16px] leading-6">Let’s move the sync to Friday at three, and I’ll send the notes before.</p>
        )}
      </Card>
      <p className="max-w-[460px] text-base text-muted-foreground">
        {polishAvailable
          ? "Voicetypr types what you meant into any app."
          : "Voicetypr types what you say into any app."}
      </p>
      <Button size="lg" disabled={busy} onClick={onBegin}>
        Continue
        <ChevronRight />
      </Button>
      <p className="text-[12.5px] text-muted-foreground">
        {nextLine}
      </p>
    </section>
  );
}
