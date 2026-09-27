import { useEffect, useRef, useCallback } from "react";
import { eventCoordinator } from "@/lib/EventCoordinator";
import type { UnlistenFn } from "@tauri-apps/api/event";

type WindowId = "main" | "pill" | "onboarding";

/**
 * Hook to easily register event handlers through the EventCoordinator
 * Automatically handles cleanup on unmount
 */
export function useEventCoordinator(windowId: WindowId) {
  const unlistenersRef = useRef<UnlistenFn[]>([]);

  useEffect(() => {
    // Set this window as active when component mounts
    eventCoordinator.setActiveWindow(windowId);

    // Cleanup function
    return () => {
      // Unregister all event listeners for this component
      unlistenersRef.current.forEach((unlisten) => unlisten());
      unlistenersRef.current = [];
    };
  }, [windowId]);

  /**
   * Register an event handler
   * @param eventName The name of the event to listen for
   * @param handler The handler function to call when event is received
   */
  const registerEvent = useCallback(
    async <T = any>(eventName: string, handler: (payload: T) => void | Promise<void>) => {
      const unlisten = await eventCoordinator.register(windowId, eventName, handler);
      unlistenersRef.current.push(unlisten);
      return unlisten;
    },
    [windowId],
  );

  const setActive = useCallback(() => {
    eventCoordinator.setActiveWindow(windowId);
  }, [windowId]);

  const getDebugInfo = useCallback(() => {
    return eventCoordinator.getDebugInfo();
  }, []);

  return {
    registerEvent,
    setActive,
    getDebugInfo,
  };
}
