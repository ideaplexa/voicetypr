import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { SettingsHeader, SettingsPage } from "@/components/settings/settings-ui";
import { useLicense } from "@/contexts/LicenseContext";
import { ask } from "@tauri-apps/plugin-dialog";
import { Crown, HelpCircle } from "lucide-react";
import { useState } from "react";
import { ActivateLicenseCard } from "./ActivateLicenseCard";
import { LicenseStatusCard } from "./LicenseStatusCard";

export function AccountSection() {
  const {
    status,
    isLoading,
    checkStatus,
    revalidateLicense,
    activateLicense,
    deactivateLicense,
    openPurchasePage,
  } = useLicense();
  const [licenseKey, setLicenseKey] = useState("");
  const [isActivating, setIsActivating] = useState(false);

  const handleActivate = async () => {
    if (!licenseKey.trim()) return;

    setIsActivating(true);
    await activateLicense(licenseKey.trim());
    setIsActivating(false);
    setLicenseKey("");
  };

  const handleDeactivate = async () => {
    const confirmed = await ask("Deactivating your license will make the app unusable.", {
      title: "Deactivate License",
      kind: "warning",
      okLabel: "Confirm",
      cancelLabel: "Cancel",
    });

    if (confirmed) {
      await deactivateLicense();
    }
  };

  const isUnlicensed =
    !isLoading &&
    (!status ||
      status.status === "expired" ||
      status.status === "none" ||
      status.status === "trial");

  return (
    <SettingsPage>
      <SettingsHeader
        title={
          <span className="flex items-center gap-2">
            License
            <Dialog>
              <DialogTrigger
                render={
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="License guide"
                    className="size-7 rounded-full text-muted-foreground"
                  />
                }
              >
                <HelpCircle className="h-4 w-4" />
              </DialogTrigger>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>License guide</DialogTitle>
                  <DialogDescription>
                    Manage your trial and activate or remove a Pro license.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                  <p>
                    <strong className="text-foreground">Trial</strong> shows the remaining trial
                    state when no Pro license is active.
                  </p>
                  <p>
                    <strong className="text-foreground">License activation</strong> validates the
                    key and stores only the app needs to confirm status.
                  </p>
                  <p>
                    <strong className="text-foreground">Purchase</strong> opens the checkout flow
                    when you need to upgrade from trial or free.
                  </p>
                </div>
              </DialogContent>
            </Dialog>
          </span>
        }
        description="Trial status, license activation, and purchase access."
        actions={
          status && status.status === "licensed" ? (
            <div className="flex items-center gap-2 rounded-lg bg-green-500/10 px-3 py-1.5">
              <Crown className="h-4 w-4 text-green-600 dark:text-green-400" />
              <span className="text-sm font-medium text-green-600 dark:text-green-400">
                Pro Licensed
              </span>
            </div>
          ) : undefined
        }
      />

      <LicenseStatusCard
        status={status}
        isLoading={isLoading}
        onCheckStatus={checkStatus}
        onRevalidate={revalidateLicense}
        onDeactivate={() => {
          void handleDeactivate();
        }}
      />

      {isUnlicensed && (
        <ActivateLicenseCard
          licenseKey={licenseKey}
          isActivating={isActivating}
          onLicenseKeyChange={setLicenseKey}
          onActivate={() => {
            void handleActivate();
          }}
          onPurchase={openPurchasePage}
        />
      )}
    </SettingsPage>
  );
}
