import { listen, UnlistenFn, Event as TauriEvent } from "@tauri-apps/api/event";
import { createLogger } from "@/lib/logger";

const log = createLogger("events");

type EventHandler<T = any> = (payload: T) => void | Promise<void>;
type WindowId = "main" | "pill" | "onboarding";

interface EventRegistration {
  windowId: WindowId;
  handler: EventHandler;
  unlisten?: UnlistenFn;
}

/**
 * Centralized event coordinator to prevent duplicate event handling
 * and ensure events are routed to the correct window.
 */
export class EventCoordinator {
  private static instance: EventCoordinator;
  private registrations: Map<string, EventRegistration[]> = new Map();
  private activeWindow: WindowId = "main";

  private constructor() {
    // Singleton pattern
  }

  static getInstance(): EventCoordinator {
    if (!EventCoordinator.instance) {
      EventCoordinator.instance = new EventCoordinator();
    }
    return EventCoordinator.instance;
  }

  /**
   * Set the currently active window for event routing
   */
  setActiveWindow(windowId: WindowId) {
    this.activeWindow = windowId;
    log.debug(`[EventCoordinator] Active window set to: ${windowId}`);
  }

  /**
   * Register an event handler for a specific window
   * Prevents duplicate registrations for the same window/event combination
   */
  async register<T = any>(
    windowId: WindowId,
    eventName: string,
    handler: EventHandler<T>,
  ): Promise<UnlistenFn> {
    // Check for existing registration and clean it up if found
    const existing = this.registrations.get(eventName);
    if (existing) {
      const existingForWindow = existing.find((reg) => reg.windowId === windowId);
      if (existingForWindow) {
        log.debug(
          `[EventCoordinator] Event "${eventName}" already registered for window "${windowId}". Cleaning up old registration.`,
        );
        // Clean up the old registration before creating a new one
        this.unregister(windowId, eventName);
      }
    }

    // Create wrapper that checks if this window should handle the event
    const wrappedHandler = (event: TauriEvent<T>) => {
      const shouldHandle = this.shouldWindowHandleEvent(windowId, eventName);

      log.debug(
        `[EventCoordinator] Event "${eventName}" received. Window: ${windowId}, Should handle: ${shouldHandle}`,
      );

      if (shouldHandle) {
        handler(event.payload);
      }
    };

    // Listen to the Tauri event
    const unlisten = await listen<T>(eventName, wrappedHandler);

    // Store registration
    const registration: EventRegistration = {
      windowId,
      handler: handler as EventHandler,
      unlisten,
    };

    if (!this.registrations.has(eventName)) {
      this.registrations.set(eventName, []);
    }
    this.registrations.get(eventName)!.push(registration);

    log.debug(`[EventCoordinator] Registered event "${eventName}" for window "${windowId}"`);

    // Return cleanup function
    return () => {
      this.unregister(windowId, eventName);
    };
  }

  /**
   * Unregister an event handler
   */
  private unregister(windowId: WindowId, eventName: string) {
    const registrations = this.registrations.get(eventName);
    if (!registrations) return;

    const index = registrations.findIndex((reg) => reg.windowId === windowId);
    if (index !== -1) {
      const registration = registrations[index];
      registration.unlisten?.();
      registrations.splice(index, 1);

      if (registrations.length === 0) {
        this.registrations.delete(eventName);
      }

      log.debug(`[EventCoordinator] Unregistered event "${eventName}" for window "${windowId}"`);
    }
  }

  /**
   * Determine if a window should handle a specific event
   * This is where routing rules are defined
   */
  private shouldWindowHandleEvent(windowId: WindowId, eventName: string): boolean {
    // Event routing rules
    const routingRules: Record<string, WindowId | "all"> = {
      // Transcription events
      "transcription-complete": "pill", // Pill window handles paste/clipboard/save
      "history-updated": "main", // Main window reloads history
      "audio-level": "pill",
      "recording-state-changed": "all",

      // Model events should go to all windows (for onboarding support)
      "download-progress": "all",
      "model-downloaded": "all",
      "model-verifying": "all",
      "download-cancelled": "all",
      "download-error": "all",

      // Recording/transcription errors now use pill_toast() → FeedbackToast directly,
      // not as routed events. Only domain-specific main window errors are listed here.
      "parakeet-unavailable": "main",

      // Debug events
      "test-event": "pill",
    };

    const rule = routingRules[eventName];

    // If no rule defined, only active window handles it
    if (!rule) {
      return windowId === this.activeWindow;
    }

    // If rule is "all", all windows handle it
    if (rule === "all") {
      return true;
    }

    // Otherwise, check if this window matches the rule
    return windowId === rule;
  }

  /**
   * Clear all registrations for a window (useful on unmount)
   */
  clearWindowRegistrations(windowId: WindowId) {
    for (const [eventName, registrations] of this.registrations.entries()) {
      const filtered = registrations.filter((reg) => {
        if (reg.windowId === windowId) {
          reg.unlisten?.();
          return false;
        }
        return true;
      });

      if (filtered.length === 0) {
        this.registrations.delete(eventName);
      } else {
        this.registrations.set(eventName, filtered);
      }
    }

    log.debug(`[EventCoordinator] Cleared all registrations for window "${windowId}"`);
  }

  /**
   * Get debug information about current registrations
   */
  getDebugInfo() {
    const info: Record<string, string[]> = {};
    for (const [eventName, registrations] of this.registrations.entries()) {
      info[eventName] = registrations.map((reg) => reg.windowId);
    }
    return {
      activeWindow: this.activeWindow,
      registrations: info,
    };
  }
}

// Export singleton instance
export const eventCoordinator = EventCoordinator.getInstance();
