import { Button } from "@/components/settings/SettingsButton";

/** After the essentials: go deeper (Polish and the rest) or finish now. */
export function ForkStep({
  onDeeper,
  onDoneForNow,
  onBack,
}: {
  onDeeper: () => void;
  onDoneForNow: () => void;
  onBack: () => void;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-[18px] text-center">
      <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">The essentials are set</h2>
      <p className="text-sm text-muted-foreground">Go deeper with Polish, or finish now and come back to it in Settings.</p>
      <div className="flex flex-wrap justify-center gap-[10px]">
        <Button variant="outline" onClick={onDeeper}>Go deeper</Button>
        <Button onClick={onDoneForNow}>Done for now</Button>
      </div>
      <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={onBack}>Back</Button>
    </section>
  );
}
