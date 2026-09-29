import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { InfoButton, PageHeader, SettingsPage } from "@/components/settings/settings-ui";
import { useLicense } from "@/contexts/LicenseContext";
import { ask } from "@tauri-apps/plugin-dialog";
import { Crown } from "lucide-react";
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
    <SettingsPage className="max-w-none gap-[18px] px-9 pb-7 pt-1 [&_button[data-slot=button]]:rounded-[10px] [&_input]:rounded-[10px]">
      <PageHeader
        title="License"
        description="Trial status, license activation, and purchase access."
        action={
          <>
            {status?.status === "licensed" ? (
              <div className="flex items-center gap-2 rounded-lg bg-sage-bg px-3 py-1.5">
                <Crown className="size-4 text-sage" />
                <span className="text-sm font-medium text-sage">Pro Licensed</span>
              </div>
            ) : null}
            <Dialog>
              <DialogTrigger render={<InfoButton label="License guide" />} />
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>License guide</DialogTitle>
                  <DialogDescription>
                    Manage your trial and activate or remove a Pro license.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                  <p>Trial shows the remaining trial state when no Pro license is active.</p>
                  <p>
                    License activation validates the key and stores only what the app needs to
                    confirm status.
                  </p>
                  <p>
                    Purchase opens the checkout flow when you need to upgrade from trial or free.
                  </p>
                </div>
              </DialogContent>
            </Dialog>
          </>
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
