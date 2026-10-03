import { Button } from "@/components/settings/SettingsButton";
import { SettingsPaneRow } from "@/components/settings/settings-ui";
import { Check } from "lucide-react";
import { isMacOS } from "@/lib/platform";
import type { LicenseStatus } from "@/types";
import { formatLicenseStatus } from "@/components/sections/accountLicenseUtils";

interface LicenseStatusCardProps {
  status: LicenseStatus | null;
  isLoading: boolean;
  onCheckStatus: () => void;
  onRevalidate: () => void;
  onDeactivate: () => void;
}
export function LicenseStatusCard({
  status,
  isLoading,
  onCheckStatus,
  onRevalidate,
  onDeactivate,
}: LicenseStatusCardProps) {
  const licensed = status?.status === "licensed";
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
                : undefined
        }
      />
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
