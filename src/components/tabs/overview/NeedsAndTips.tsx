import type { ComponentType } from "react";
import { AudioLines, BadgeCheck, BookA, Download, Mic, RotateCcw, ShieldAlert, Sparkles } from "lucide-react";
import { Button } from "@/components/settings/SettingsButton";
import type { NeedRow, TipRow } from "@/lib/home-needs";

const needIcons: Record<NeedRow["icon"], ComponentType<{ className?: string }>> = {
  retry: RotateCcw,
  update: Download,
  license: BadgeCheck,
  microphone: Mic,
  accessibility: ShieldAlert,
  source: AudioLines,
};
const tipIcons: Record<TipRow["id"], ComponentType<{ className?: string }>> = { polish: Sparkles, dictionary: BookA };

function Group({ id, title, children }: { id: string; title: string; children: React.ReactNode }) {
  return (
    <section aria-labelledby={id} className="flex flex-col gap-2.5">
      <h2 id={id} className="text-[14px] leading-[normal] font-semibold text-foreground">{title}</h2>
      <ul className="divide-y divide-border rounded-[12px] border border-border bg-card">{children}</ul>
    </section>
  );
}

function Row({ icon: Icon, title, description, button, disabled, onClick, secondary }: { icon: ComponentType<{ className?: string }>; title: string; description: string; button: string; disabled?: boolean; onClick: () => void; secondary?: React.ReactNode }) {
  return (
    <li className="flex items-center gap-3.5 px-4 py-3">
      <Icon aria-hidden className="size-4 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <p className="text-[13px] leading-[normal] font-medium text-foreground">{title}</p>
        <p className="mt-0.5 text-xs leading-[normal] text-muted-foreground">{description}</p>
      </div>
      <Button variant="outline" size="sm" disabled={disabled} onClick={onClick} aria-label={`${button}: ${title}`}>{button}</Button>
      {secondary}
    </li>
  );
}

export function NeedsYou({ rows, onAct }: { rows: NeedRow[]; onAct: (row: NeedRow) => void }) {
  if (rows.length === 0) return null;
  return (
    <Group id="needs-you-title" title="Needs you">
      {rows.map((row) => <Row key={row.id} icon={needIcons[row.icon]} title={row.title} description={row.description} button={row.button} disabled={row.disabled} onClick={() => onAct(row)} secondary={row.discard && <Button variant="ghost" size="sm" aria-label={`Discard: ${row.title}`} onClick={() => { if(row.discard) onAct({...row,action:row.discard}); }}>Discard</Button>} />)}
    </Group>
  );
}

export function MoreFromVoicetypr({ tips, onOpen }: { tips: TipRow[]; onOpen: (tip: TipRow) => void }) {
  if (tips.length === 0) return null;
  return (
    <Group id="get-more-title" title="Get more out of Voicetypr">
      {tips.map((tip) => <Row key={tip.id} icon={tipIcons[tip.id]} title={tip.title} description={tip.description} button={tip.button} onClick={() => onOpen(tip)} />)}
    </Group>
  );
}
