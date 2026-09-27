import { useEffect, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createLogger } from "@/lib/logger";

const log = createLogger("tauri-event");

/**
 * Subscribe to a Tauri event for the lifetime of the component.
 *
 * The handler always sees the latest closure (kept in a ref), so the
 * subscription itself is created exactly once per event name — no
 * resubscribe churn, no dependency arrays to get wrong.
 */
export function useTauriEvent<T = unknown>(
  event: string,
  handler: (payload: T) => void | Promise<void>,
) {
  const handlerRef = useRef(handler);

  useEffect(() => {
    handlerRef.current = handler;
  });

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;

    void listen<T>(event, (e) => void handlerRef.current(e.payload))
      .then((fn) => {
        if (disposed) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch((error) => {
        log.error(`Failed to listen for "${event}":`, error);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [event]);
}
