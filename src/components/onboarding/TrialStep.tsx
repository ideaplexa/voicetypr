import { TrialStartForm } from "@/components/TrialStartForm";
import { Button } from "@/components/settings/SettingsButton";
import type { TrialStartError } from "@/types";
import { TermsLine } from "@/components/onboarding/ConsentChoice";

/** After "Start 7-day trial": the name and email the trial is started with. */
export function TrialStep({
  pending,
  onStartTrial,
  onBack,
}: {
  /** A trial start is in flight: leaving now would let it be sent twice. */
  pending: boolean;
  onStartTrial: (name: string, email: string) => Promise<TrialStartError | null>;
  onBack: () => void;
}) {
  return (
    <section className="flex w-full flex-col items-center gap-5">
      <h2 className="text-center text-[28px] leading-[normal] font-semibold tracking-[-0.5px]">Start your 7 days free</h2>
      <p className="text-center text-sm text-muted-foreground">No card. The clock starts when you click, not before.</p>
      <TrialStartForm large onStart={onStartTrial} className="w-full max-w-[420px] gap-4" />
      <p className="flex flex-wrap items-center justify-center gap-1 text-xs text-muted-foreground">
        <TermsLine action="starting" />
      </p>
      <Button variant="ghost" size="sm" className="text-muted-foreground" disabled={pending} onClick={onBack}>Back</Button>
    </section>
  );
}
