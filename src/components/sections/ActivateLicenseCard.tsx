import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SettingsCard } from "@/components/settings/settings-ui";
import { Crown } from "lucide-react";
import { openExternalLink } from "./accountLicenseUtils";

interface ActivateLicenseCardProps {
  licenseKey: string;
  isActivating: boolean;
  onLicenseKeyChange: (value: string) => void;
  onActivate: () => void;
  onPurchase: () => void;
}

export function ActivateLicenseCard({
  licenseKey,
  isActivating,
  onLicenseKeyChange,
  onActivate,
  onPurchase,
}: ActivateLicenseCardProps) {
  return (
    <SettingsCard
      icon={Crown}
      title="Activate license"
      description="Upgrade to Pro or enter an existing license key."
    >
      <div className="mt-4 space-y-4">
        <div className="flex gap-2">
          <Button onClick={onPurchase} className="flex-1" size="sm">
            <Crown className="mr-1.5 h-3.5 w-3.5" />
            Buy License
          </Button>
          <Button
            onClick={() => openExternalLink("https://polar.sh/ideaplexa/portal")}
            variant="outline"
            size="sm"
            className="flex-1"
          >
            Manage License
          </Button>
        </div>

        <div className="space-y-2 border-t border-border/50 pt-4">
          <p className="text-sm font-medium">Have a license key?</p>
          <div className="flex gap-2">
            <Input
              placeholder="Enter license key"
              value={licenseKey}
              onChange={(e) => onLicenseKeyChange(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  onActivate();
                }
              }}
              className="flex-1 text-sm"
            />
            <Button onClick={onActivate} disabled={!licenseKey.trim() || isActivating} size="sm">
              {isActivating ? "Activating..." : "Activate"}
            </Button>
          </div>
          <p className="text-xs text-muted-foreground">
            You may be prompted for your password to securely store the license
          </p>
        </div>
      </div>
    </SettingsCard>
  );
}
