import appIcon from "../../../src-tauri/icons/128x128@2x.png";
import { Button } from "@/components/settings/SettingsButton";
import { Card } from "@/components/ui/card";
import { Band, BandWord } from "@/components/onboarding/OnboardingChrome";
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
    <section className="flex w-full flex-col items-center gap-[22px] text-center">
      <Band>
        {polishAvailable ? (
          <>
            <BandWord struck className="-left-[170px] -top-6">um</BandWord>
            <BandWord struck className="-left-[210px] top-5">like</BandWord>
            <BandWord struck className="-left-[100px] top-6">uh</BandWord>
          </>
        ) : (
          <>
            <BandWord className="-left-[190px] -top-6">Let’s move</BandWord>
            <BandWord className="-left-[130px] top-6">the sync</BandWord>
          </>
        )}
        <AppMark />
        <BandWord className="left-[90px] -top-6">Friday</BandWord>
        <BandWord className="left-[150px] top-6">at three.</BandWord>
      </Band>
      <h1 className="text-[46px] leading-[1.05] font-semibold tracking-[-1.6px] text-balance">
        {polishAvailable ? <>Say it messy.<br />It lands clean.</> : <>Say it.<br />It lands in any app.</>}
      </h1>
      <Card className="w-full max-w-[520px] gap-2 px-5 py-4 text-left">
        <span className="text-[11.5px] font-medium tracking-wide text-muted-foreground uppercase">Example</span>
        {polishAvailable ? (
          <p className="text-[16px] leading-6">
            <s className="text-muted-foreground">um, so</s> Let’s move the sync to{" "}
            <s className="text-muted-foreground">Thursday, no wait,</s> Friday at 3.
          </p>
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
