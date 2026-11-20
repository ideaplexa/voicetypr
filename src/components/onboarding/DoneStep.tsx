import trayIcon from "../../../src-tauri/icons/tray-light.png";
import { FrameHeading, FrameKeys, PrimaryButton, QuietButton } from "@/components/onboarding/Frame";
import { WhatsShared } from "@/components/WhatsShared";
import { isMacOS } from "@/lib/platform";
import { BatteryFull, Compass, Search, Wifi } from "lucide-react";

/** 13 Done (13b after buying or for a device that already has access): where Voicetypr lives and the shortcut. */
export function DoneStep({
  title,
  line,
  caps,
  holdToTalk,
  saving,
  onFinish,
  onSeeCapabilities,
}: {
  title: string;
  /** What access they have, before where Voicetypr lives. */
  line: string;
  caps: string[];
  holdToTalk: boolean;
  saving: boolean;
  onFinish: () => void;
  /** Finishes setup and opens the capabilities page. */
  onSeeCapabilities: () => void;
}) {
  const now = new Date();
  const clock = now.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false });
  return (
    <section className="flex w-[620px] flex-col items-center gap-[34px]">
      <FrameHeading
        title={title}
        line={`${line} Voicetypr stays in your ${isMacOS ? "menu bar" : "system tray"} when this window closes.`}
      />
      <div aria-hidden="true" className="flex h-14 items-center justify-end gap-5 self-stretch rounded-[14px] border border-border bg-card px-5 text-text-3">
        <Wifi className="size-[18px]" />
        <BatteryFull className="size-[18px]" />
        <Search className="size-[18px]" />
        <span className="rounded-lg bg-sage-bg p-1.5">
          <img src={trayIcon} alt="" className="size-[22px]" />
        </span>
        <span className="text-[14.5px]">
          {isMacOS ? `${now.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" })} ${clock}` : clock}
        </span>
      </div>
      <p className="flex items-center gap-3 text-[17px] text-muted-foreground">
        {holdToTalk ? "Hold" : "Press"}
        <FrameKeys caps={caps} />
        in any app to dictate.
      </p>
      <div className="flex flex-col items-center gap-3">
        <PrimaryButton disabled={saving} onClick={onFinish}>Start using Voicetypr</PrimaryButton>
        <QuietButton className="text-sage hover:text-sage" disabled={saving} onClick={onSeeCapabilities}>
          <Compass aria-hidden="true" className="size-[15px]" />
          See what Voicetypr can do
        </QuietButton>
      </div>
    </section>
  );
}

/** The crash-report choice at the foot of Done: asked once, off by default. */
export function DoneConsent({ checked, disabled, onChange }: { checked: boolean; disabled: boolean; onChange: (checked: boolean) => void }) {
  return (
    <div className="flex items-center gap-2 text-[13.5px] text-muted-foreground">
      <label className="flex items-center gap-2">
        <input
          type="checkbox"
          checked={checked}
          disabled={disabled}
          onChange={(event) => onChange(event.target.checked)}
          className="size-[15px] accent-[var(--sage)]"
        />
        Share anonymous crash reports and usage numbers
      </label>
      <WhatsShared />
    </div>
  );
}
