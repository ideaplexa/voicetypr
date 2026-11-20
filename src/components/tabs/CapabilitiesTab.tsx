import { ArrowLeft, ArrowRight } from "lucide-react";
import { DiscoveryArt } from "@/components/discovery/DiscoveryArt";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/settings/SettingsButton";
import { PageHeader, SettingsPage } from "@/components/settings/settings-ui";
import { useSettings } from "@/contexts/SettingsContext";
import { useRealtimeWords } from "@/components/sections/models/useRealtimeWords";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import { cn } from "@/lib/utils";
import { usePolishCanRun } from "@/lib/polish-ready";
import { capabilityGroups, type Capability } from "@/components/capabilities/capabilityCatalog";

export function CapabilitiesTab({ onNavigate, onNavigateSettingsPane }: { onNavigate?: (screen: ScreenId) => void; onNavigateSettingsPane?: (pane: SettingsPane) => void }) {
  const { settings } = useSettings();
  const polishReady = usePolishCanRun();
  const { streams } = useRealtimeWords(settings?.current_model_engine ?? "whisper", settings?.current_model ?? "");
  const open = (item: Capability) => {
    if (!item.target) return;
    if ("pane" in item.target) onNavigateSettingsPane?.(item.target.pane);
    else onNavigate?.(item.target.screen);
  };
  return (
    <SettingsPage wide container className="gap-7">
      <div className="flex flex-col gap-2">
        <Button variant="ghost" size="sm" className="-ml-2 w-fit text-muted-foreground" onClick={() => onNavigate?.("help")}><ArrowLeft className="size-3.5" />Help & feedback</Button>
        <PageHeader title="What Voicetypr can do" description="Each card says what it needs and opens the place where it lives." />
      </div>
      {capabilityGroups(streams, polishReady === true).map((group) => (
        <section key={group.title} aria-labelledby={`capability-${group.title.toLowerCase().replace(/\W+/g, "-")}`} className="flex flex-col gap-3">
          <h2 id={`capability-${group.title.toLowerCase().replace(/\W+/g, "-")}`} className="text-[14px] leading-[normal] font-semibold text-foreground">{group.title}</h2>
          <ul className="grid gap-3 @lg:grid-cols-2 @2xl:grid-cols-3">
            {group.items.map((item) => (
              <li key={item.id} data-capability={item.id} className="flex flex-col gap-3 rounded-[14px] border border-border bg-card p-3">
                <DiscoveryArt kind={item.art} className="h-[88px] w-full" />
                <div className="flex flex-1 flex-col items-start gap-1.5 px-1 pb-1">
                  <h3 className="text-[13.5px] leading-[normal] font-semibold text-foreground">{item.title}</h3>
                  <Badge variant="secondary" className={cn("font-medium", item.badge === "Works now" ? "bg-sage-bg text-sage" : "bg-muted text-muted-foreground")}>{item.badge}</Badge>
                  <p className="text-[12.5px] leading-[18px] text-muted-foreground">{item.line}</p>
                  {item.target ? <Button variant="link" size="sm" className="mt-auto h-auto px-0 pt-1 text-sage" onClick={() => open(item)}>{item.target.label}<ArrowRight className="size-3.5" /></Button> : null}
                </div>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </SettingsPage>
  );
}
