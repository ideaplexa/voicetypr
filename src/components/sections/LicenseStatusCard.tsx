import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
        title="Status"
        control={
          <Badge variant={getStatusBadgeVariant(status)} className="font-medium">
            {formatLicenseStatus(status, isLoading)}
          </Badge>
        }
      />

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
          <div className="mt-4 rounded-lg border border-amber-500/30 bg-amber-500/10 p-4">
            <div className="flex items-start gap-3">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600 dark:text-amber-400" />
              <div className="min-w-0 flex-1 space-y-1">
                <p className="text-sm font-medium text-amber-900 dark:text-amber-200">
                  {status.verification_state === "needs_revalidation"
                    ? "License verification still unavailable"
                    : "Couldn’t verify license"}
                </p>
                <p className="text-xs text-amber-800 dark:text-amber-300">
                  Offline access remains available. Your paid license has not expired.
                </p>
                {status.expires_at && (
                  <p className="text-xs text-muted-foreground">{status.expires_at}</p>
                )}
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
          <div className="space-y-3 rounded-lg border border-green-500/20 bg-green-500/10 p-4">
            <div className="flex items-start gap-3">
              <div className="rounded-md bg-green-500/10 p-1.5">
                <Check className="h-4 w-4 text-green-600 dark:text-green-400" />
              </div>
              <div className="flex-1 space-y-1">
                <p className="text-sm font-medium text-green-900 dark:text-green-100">
                  Voicetypr Pro Active
                </p>
                {status.license_key && (
                  <p className="font-mono text-xs text-green-700 dark:text-green-300">
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
        <div className="mt-4 rounded-lg border border-amber-500/20 bg-amber-500/10 p-4">
          <div className="flex items-start gap-3">
            <div className="rounded-md bg-amber-500/10 p-1.5">
              <Clock className="h-4 w-4 text-amber-500" />
            </div>
            <div className="flex-1 space-y-1">
              <p className="text-sm font-medium text-amber-900 dark:text-amber-400">
                {status.status === "trial" ? "Trial Active" : "Trial Expired"}
              </p>
              <p className="text-xs text-amber-800 dark:text-amber-500">
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
