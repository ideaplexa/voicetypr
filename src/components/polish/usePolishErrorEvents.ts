import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useEnhancementsStore, type PolishErrorKind } from "@/state/enhancements";
import { toast } from "sonner";

export function usePolishErrorEvents() {
  const setPolishError = useEnhancementsStore((s) => s.setPolishError);
  const clearPolishError = useEnhancementsStore((s) => s.clearPolishError);

  useTauriEvent<{ category?: string; message?: string } | null>("enhancing-failed", (payload) => {
    if (!payload || payload.category === "canceled") return;
    // Only local agent CLI failures surface their raw message as a toast.
    if (payload.category === "cli_error" && typeof payload.message === "string" && payload.message.trim()) {
      toast.error(payload.message);
    }
    const kind: PolishErrorKind =
      payload.category === "missing_api_key" || payload.category === "invalid_api_key"
        ? "auth"
        : "generic";
    setPolishError(kind, payload.message || "Polish failed");
  });

  useTauriEvent("enhancing-completed", () => {
    clearPolishError();
  });
}
