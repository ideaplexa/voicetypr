import { PageHeader, SettingsPage } from "@/components/settings/settings-ui";
import { useLicense } from "@/contexts/LicenseContext";
import { ask } from "@tauri-apps/plugin-dialog";

import { useState } from "react";
import { ActivateLicenseCard } from "@/components/sections/ActivateLicenseCard";
import { LicenseStatusCard } from "@/components/sections/LicenseStatusCard";

export function AccountSection({ embedded = false }: { embedded?: boolean } = {}) {
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
    <SettingsPage
      className={`${embedded ? "!p-0" : ""} gap-[18px] [&_button[data-slot=button]]:rounded-[10px] [&_input]:rounded-[10px]`}
    >
      {!embedded && (
        <PageHeader title="License" description="Trial status and lifetime Pro license." />
      )}

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
