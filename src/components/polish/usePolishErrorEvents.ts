import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useEnhancementsStore, type PolishErrorKind } from "@/state/enhancements";

export function usePolishErrorEvents() {
  const setPolishError = useEnhancementsStore((s) => s.setPolishError);
  const clearPolishError = useEnhancementsStore((s) => s.clearPolishError);

  useTauriEvent<unknown>("ai-enhancement-auth-error", (payload) => {
    if (typeof payload === "string") {
      setPolishError("auth", payload);
    }
  });

  useTauriEvent<unknown>("ai-enhancement-error", (payload) => {
    if (typeof payload === "string") {
      setPolishError("generic", payload);
    }
  });

  useTauriEvent<{ category?: string; message?: string } | null>("enhancing-failed", (payload) => {
    if (!payload || payload.category === "canceled") return;
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
