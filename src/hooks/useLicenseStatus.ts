import { useLicense } from "@/contexts/LicenseContext";

/**
 * A thin wrapper around useLicense that provides derived values for readiness checks.
 * This hook simply uses the LicenseContext data and adds computed properties.
 */
export function useLicenseStatus() {
  const { status, isLoading, checkStatus } = useLicense();

  // Derive the isValid computed value
  const isValid = status ? ["licensed", "trial"].includes(status.status) : false;

  return {
    licenseStatus: status,
    isValid,
    isChecking: isLoading,
    checkLicense: checkStatus,
  };
}
