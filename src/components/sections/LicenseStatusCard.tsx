import { Button } from "@/components/settings/SettingsButton";
import { SettingsPaneRow } from "@/components/settings/settings-ui";
import { Check } from "lucide-react";
import { isMacOS } from "@/lib/platform";
import type { LicenseStatus, TrialStartError } from "@/types";
import { TrialStartForm } from "@/components/TrialStartForm";
import { formatLicenseStatus } from "@/components/sections/accountLicenseUtils";

interface LicenseStatusCardProps {
  status: LicenseStatus | null;
  isLoading: boolean;
  onCheckStatus: () => void;
  onRevalidate: () => void;
  onStartTrial: (name: string, email: string) => Promise<TrialStartError | null>;
  onDeactivate: () => void;
}
export function LicenseStatusCard({
  status,
  isLoading,
  onCheckStatus,
  onRevalidate,
  onStartTrial,
  onDeactivate,
}: LicenseStatusCardProps) {
  const licensed = status?.status === "licensed";
  const trialNotStarted = status?.status === "none";
  return (
    <section>
      <SettingsPaneRow
        title={
          licensed ? (
            <span className="flex items-center gap-2">
              <Check className="size-4 text-sage" />
              Voicetypr Pro · lifetime
            </span>
          ) : (
            formatLicenseStatus(status, isLoading)
          )
        }
        description={
          !isLoading && !status
            ? "Couldn’t load license status"
            : licensed && status.verification_state && status.verification_state !== "verified"
              ? "Couldn’t verify your license. Connect to the internet, then recheck."
              : status?.status === "trial"
                ? `${status.trial_days_left ?? 0} days remaining in your trial`
                : trialNotStarted
                  ? "All of Voicetypr, free for 7 days. No card. The clock starts when you click, not before."
                  : undefined
        }
      />
      {trialNotStarted && <TrialStartForm onStart={onStartTrial} className="items-start pb-3" />}
      <div className="flex flex-wrap gap-2 border-t border-border pt-3">
        <Button
          variant="secondary"
          size="sm"
          disabled={isLoading}
          onClick={licensed ? onRevalidate : onCheckStatus}
        >
          Recheck
        </Button>
        {licensed && (
          <Button
            variant="ghost"
            size="sm"
            className="text-destructive hover:text-destructive"
            onClick={onDeactivate}
          >
            Deactivate on this {isMacOS ? "Mac" : "PC"}
          </Button>
        )}
      </div>
    </section>
  );
}
