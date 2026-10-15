import { useSyncExternalStore } from "react";
import { updateService } from "@/services/updateService";

/** Live update state from the single UpdateService, for any surface that shows it. */
export function useUpdateStatus() {
  const status = useSyncExternalStore(updateService.subscribe, updateService.getStatus);
  return {
    ...status,
    checkNow: () => updateService.checkNow(),
    promptInstall: () => updateService.promptInstall(),
  };
}
