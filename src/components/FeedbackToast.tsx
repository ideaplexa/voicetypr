import { listen } from "@tauri-apps/api/event";
import { Info, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

export type PillToastAction = "show" | "clear";
export type PillToastVariant = "info" | "warning";

export interface PillToastPayload {
  id: number;
  message: string;
  duration_ms: number;
  action?: PillToastAction;
  variant?: PillToastVariant;
  persistent?: boolean;
  suggestion?: string;
}

type ToastSeverity = "info" | "success" | "error" | "warning";

interface ActiveToast {
  id: number;
  message: string;
  severity: ToastSeverity;
  suggestion?: string;
}

function inferSeverity(message: string): ToastSeverity {
  const normalized = message.toLowerCase();
  if (
    normalized.includes("error") ||
    normalized.includes("fail") ||
    normalized.includes("unable") ||
    normalized.includes("couldn't")
  ) {
    return "error";
  }
  if (
    normalized.includes("complete") ||
    normalized.includes("copied") ||
    normalized.includes("ready") ||
    normalized.includes("saved")
  ) {
    return "success";
  }
  return "info";
}

function severityForPayload({ message, variant }: PillToastPayload): ToastSeverity {
  if (variant === "warning") return "warning";
  if (variant === "info") return "info";
  return inferSeverity(message);
}

export function FeedbackToast() {
  const [toast, setToast] = useState<ActiveToast | null>(null);
  const latestIdRef = useRef(Number.NEGATIVE_INFINITY);
  const timerRef = useRef<number | null>(null);

  const clearTimer = useCallback(() => {
    if (!timerRef.current) return;
    clearTimeout(timerRef.current);
    timerRef.current = null;
  }, []);

  const applyPayload = useCallback(
    (payload: PillToastPayload) => {
      const action = payload.action ?? "show";

      if (action === "clear") {
        if (payload.id !== latestIdRef.current) return;
        clearTimer();
        setToast(null);
        return;
      }

      if (payload.id < latestIdRef.current) return;

      latestIdRef.current = payload.id;
      clearTimer();
      setToast({
        id: payload.id,
        message: payload.message,
        severity: severityForPayload(payload),
        suggestion: payload.suggestion,
      });

      if (payload.persistent === true) return;

      timerRef.current = window.setTimeout(() => {
        if (latestIdRef.current !== payload.id) return;
        setToast(null);
        timerRef.current = null;
      }, payload.duration_ms);
    },
    [clearTimer],
  );

  useEffect(() => {
    let isMounted = true;
    let unlistenFn: (() => void) | undefined;

    void listen<PillToastPayload>("toast", (evt) => {
      if (isMounted) applyPayload(evt.payload);
    }).then((unlisten) => {
      if (!isMounted) {
        unlisten();
        return;
      }
      unlistenFn = unlisten;
    });

    return () => {
      isMounted = false;
      unlistenFn?.();
      clearTimer();
    };
  }, [applyPayload, clearTimer]);

  if (!toast) {
    return null;
  }

  const isAlert = toast.severity === "warning" || toast.severity === "error";

  return (
    <div className="pointer-events-none fixed inset-0 flex items-center justify-center">
      <div
        role="status"
        aria-live="polite"
        className={`flex min-w-[200px] max-w-[400px] items-start gap-2 rounded-lg px-4 py-2 text-sm shadow-lg ring-1 ${
          isAlert
            ? "bg-amber-950 text-amber-50 ring-amber-400/40"
            : "bg-black text-white ring-white/30"
        }`}
      >
        {isAlert ? (
          <TriangleAlert className="mt-0.5 h-4 w-4 flex-shrink-0 text-amber-300" aria-hidden />
        ) : (
          <Info className="mt-0.5 h-4 w-4 flex-shrink-0 text-white/80" aria-hidden />
        )}
        <span
          aria-hidden
          className={`flex-shrink-0 ${isAlert ? "text-amber-400/50" : "text-white/30"}`}
        >
          |
        </span>
        <div className="flex min-w-0 flex-col">
          <span className="overflow-hidden break-words whitespace-pre-wrap [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:2]">
            {toast.message}
          </span>
          {toast.suggestion && (
            <span
              className={`mt-0.5 overflow-hidden break-words text-xs [display:-webkit-box] [-webkit-box-orient:vertical] [-webkit-line-clamp:2] ${
                isAlert ? "text-amber-200/70" : "text-white/60"
              }`}
            >
              {toast.suggestion}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
