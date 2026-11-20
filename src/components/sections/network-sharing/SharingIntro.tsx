import { Check } from "lucide-react";
import { SettingsPaneCard } from "@/components/settings/settings-ui";
import { SectionIntro, TRUSTED_NETWORK_WARNING } from "@/components/discovery/SectionIntro";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";

const computer = isMacOS ? "Mac" : "PC";

/** The two directions sharing works in, and where each is set. One control: the switch below. */
export function SharingIntro() {
  return (
    <>
      <SectionIntro
        art="share"
        line={<>Share this {computer} with the switch below, and your other computers can use it as their engine. To use another computer instead, open Transcription → Another computer.</>}
      />
      <p role="note" className="rounded-[10px] border border-warn/25 bg-warn-bg px-3.5 py-2.5 text-[12.5px] leading-[18px] text-warn">{TRUSTED_NETWORK_WARNING}</p>
    </>
  );
}

/** Sharing off: what this computer still needs, from its real state. */
export function SharingNeeds({ activeRemoteServer, hasShareableModel, currentSelectionShareable }: { activeRemoteServer: boolean; hasShareableModel: boolean; currentSelectionShareable: boolean }) {
  const modelReady = !activeRemoteServer && hasShareableModel && currentSelectionShareable;
  const modelHint = activeRemoteServer ? "Stop using another computer first" : !hasShareableModel ? "Download one in Transcription" : !currentSelectionShareable ? "Select it in Transcription" : "Ready";
  const rows: [ok: boolean | null, need: string, hint: string][] = [
    [modelReady, "A downloaded on-device model, selected here", modelHint],
    [null, "Another computer with Voicetypr", "Mac or PC"],
    [null, "The same network", "Wi-Fi or cable"],
  ];
  return (
    <SettingsPaneCard title="Sharing is off">
      <p className="text-[12.5px] text-muted-foreground">To share this {computer}, you need:</p>
      <ul className="mt-2 mb-2">
        {rows.map(([ok, need, hint]) => (
          <li key={need} className="flex items-center gap-3 border-t border-border py-2.5 text-[13px] text-foreground">
            <span aria-hidden className={cn("flex size-[18px] shrink-0 items-center justify-center rounded-full", ok ? "bg-sage text-sage-foreground" : "border-[1.5px] border-border")}>{ok ? <Check className="size-3" /> : null}</span>
            <span className="min-w-0 flex-1">{need}<span className="sr-only">{ok ? ": ready" : ok === false ? ": not ready" : ""}</span></span>
            <span className="text-xs text-muted-foreground">{hint}</span>
          </li>
        ))}
      </ul>
    </SettingsPaneCard>
  );
}
