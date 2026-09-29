import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/settings/SettingsButton";
import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import { AlertTriangle, Check, Clock, RefreshCw, Shield } from "lucide-react";
import type { LicenseStatus } from "@/types";
import {
  formatLicenseStatus,
  getStatusBadgeVariant,
  openExternalLink,
} from "./accountLicenseUtils";

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
  return (
    <SettingsCard
      icon={Shield}
      title="License status"
      description="Your current trial or Pro license state."
    >
      <SettingRow
        title="Plan"
        control={
          <span className="text-sm font-medium text-foreground">
            {isLoading
              ? "Loading..."
              : status?.status === "licensed"
                ? status.license_type || "Pro"
                : status?.status === "trial"
                  ? "Trial"
                  : "No plan"}
          </span>
        }
      />
      <SettingRow
        title="Status"
        control={
          <Badge
            variant={getStatusBadgeVariant(status)}
            className="bg-muted text-muted-foreground font-medium"
          >
            {formatLicenseStatus(status, isLoading)}
          </Badge>
        }
      />

      {status?.expires_at ? (
        <SettingRow
          title="Expiry / offline access"
          control={
            <span className="font-mono text-xs text-muted-foreground">{status.expires_at}</span>
          }
        />
      ) : null}
      {!isLoading && !status && (
        <SettingRow
          title="Couldn’t load license status"
          description="We weren’t able to read your license. Try again."
          control={
            <Button onClick={onCheckStatus} variant="outline" size="sm">
              Retry
            </Button>
          }
        />
      )}

      {status?.status === "licensed" &&
        status.verification_state &&
        status.verification_state !== "verified" && (
          <div className="mt-4 rounded-lg border border-border bg-warn-bg p-4">
            <div className="flex items-start gap-3">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warn" />
              <div className="min-w-0 flex-1 space-y-1">
                <p className="text-sm font-medium text-warn">
                  {status.verification_state === "needs_revalidation"
                    ? "License verification still unavailable"
                    : "Couldn’t verify license"}
                </p>
                <p className="text-xs text-muted-foreground">
                  Offline access remains available. Your paid license has not expired.
                </p>
              </div>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={onRevalidate}
                disabled={isLoading}
              >
                <RefreshCw className={isLoading ? "animate-spin" : undefined} />
                Revalidate now
              </Button>
            </div>
          </div>
        )}

      {status && status.status === "licensed" && (
        <div className="mt-4 space-y-4">
          <div className="space-y-3 rounded-lg border border-border bg-sage-bg p-4">
            <div className="flex items-start gap-3">
              <div className="rounded-md bg-sage-bg p-1.5">
                <Check className="h-4 w-4 text-sage" />
              </div>
              <div className="flex-1 space-y-1">
                <p className="text-sm font-medium text-foreground">Voicetypr Pro Active</p>
                {status.license_key && (
                  <p className="font-mono text-xs text-muted-foreground">
                    License: ****-****-****-{status.license_key.slice(-4)}
                  </p>
                )}
                <p className="mt-2 text-xs text-muted-foreground">All pro features unlocked</p>
              </div>
            </div>
          </div>

          <div className="flex gap-2">
            {status.verification_state === "verified" && (
              <Button
                onClick={onRevalidate}
                disabled={isLoading}
                variant="outline"
                size="sm"
                className="flex-1"
              >
                <RefreshCw className={isLoading ? "animate-spin" : undefined} />
                Revalidate License
              </Button>
            )}
            <Button
              onClick={() => openExternalLink("https://polar.sh/ideaplexa/portal")}
              variant="outline"
              size="sm"
              className="flex-1"
            >
              Manage License
            </Button>
            <Button onClick={onDeactivate} variant="outline" size="sm" className="flex-1">
              Deactivate License
            </Button>
          </div>
        </div>
      )}

      {status && (status.status === "trial" || status.status === "expired") && (
        <div className="mt-4 rounded-lg border border-border bg-warn-bg p-4">
          <div className="flex items-start gap-3">
            <div className="rounded-md bg-warn-bg p-1.5">
              <Clock className="h-4 w-4 text-warn" />
            </div>
            <div className="flex-1 space-y-1">
              <p className="text-sm font-medium text-warn">
                {status.status === "trial" ? "Trial Active" : "Trial Expired"}
              </p>
              <p className="text-xs text-muted-foreground">
                {status.status === "trial" && status.trial_days_left !== undefined
                  ? status.trial_days_left > 0
                    ? `${status.trial_days_left} day${status.trial_days_left !== 1 ? "s" : ""} remaining in your trial`
                    : "Trial expires today"
                  : "Upgrade to Pro to continue"}
              </p>
            </div>
          </div>
        </div>
      )}
    </SettingsCard>
  );
}
