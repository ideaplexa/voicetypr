import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Wrench } from "lucide-react";
import type { ScreenId } from "@/components/navigation";

function SetupValue({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 border-t border-border/70 pt-4 first:border-t-0 first:pt-0 sm:border-t-0 sm:pt-0">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className="mt-1 truncate text-sm font-semibold text-foreground">{value}</dd>
    </div>
  );
}

export function CurrentSetupCard({
  canRecord,
  onNavigate,
  selectedSourceLabel,
  triggerLabel,
  spokenLanguage,
  canAutoInsert,
}: {
  canRecord: boolean;
  onNavigate?: (section: ScreenId) => void;
  selectedSourceLabel: string;
  triggerLabel: string;
  spokenLanguage: string;
  canAutoInsert: boolean;
}) {
  return (
    <section className="rounded-xl bg-card p-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="text-[15px] font-semibold text-foreground">Current setup</h2>
          <p className="mt-1 text-[13px] text-muted-foreground">
            Defaults used for your next desktop recording.
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {!canRecord && onNavigate ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => onNavigate("advanced")}
            >
              <Wrench />
              Quick help
            </Button>
          ) : null}
          <span
            className={cn(
              "rounded-full px-2.5 py-1 text-xs font-medium",
              canRecord
                ? "bg-sage-bg text-sage"
                : "bg-amber-500/10 text-amber-700 dark:text-amber-300",
            )}
          >
            {canRecord ? "Ready" : "Needs attention"}
          </span>
        </div>
      </div>
      <dl className="mt-5 grid gap-x-8 gap-y-5 sm:grid-cols-2">
        <SetupValue label="Source" value={selectedSourceLabel} />
        <SetupValue
          label="Recording shortcut"
          value={triggerLabel === "Not set" ? "Not configured" : triggerLabel}
        />
        <SetupValue label="Spoken language" value={spokenLanguage} />
        <SetupValue
          label="After recording"
          value={canAutoInsert ? "Insert at the cursor" : "Keep in History"}
        />
      </dl>
    </section>
  );
}
