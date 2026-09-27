export interface DistributionInfo {
  channel: "direct" | "store_msix";
  is_store_install: boolean;
  package_family_name: string | null;
}

export function isStoreDistribution(info: DistributionInfo | null | undefined): boolean {
  return info?.is_store_install === true;
}
