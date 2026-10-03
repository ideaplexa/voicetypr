import { StrictMode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import type { Event } from "@tauri-apps/api/event";
import { listen } from "@tauri-apps/api/event";
import { eventCoordinator } from "@/lib/EventCoordinator";
import { useAppEvents } from "./useAppEvents";

const listeners = new Map<string, Set<(event: Event<unknown>) => void>>();
const pendingOverview: Array<() => void> = [];

vi.mock("@/components/polish/usePolishErrorEvents", () => ({ usePolishErrorEvents: () => {} }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), warning: vi.fn() } }));

function deferredOverviewListen(eventName: string, handler: (event: Event<unknown>) => void) {
  const active = listeners.get(eventName) ?? new Set<(event: Event<unknown>) => void>();
  listeners.set(eventName, active);
  const attach = () => {
    active.add(handler);
    return () => {
      active.delete(handler);
    };
  };
  if (eventName !== "navigate-to-overview") return Promise.resolve(attach());
  return new Promise<() => void>((resolve) => {
    pendingOverview.push(() => resolve(attach()));
  });
}

describe("useAppEvents registration lifecycle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    listeners.clear();
    pendingOverview.length = 0;
    eventCoordinator.clearWindowRegistrations("main");
    eventCoordinator.setActiveWindow("main");
    vi.mocked(listen).mockImplementation((eventName, handler) =>
      deferredOverviewListen(eventName, handler as (event: Event<unknown>) => void),
    );
  });

  afterEach(() => eventCoordinator.clearWindowRegistrations("main"));

  it("keeps the replacement StrictMode listener when registrations resolve out of order", async () => {
    const setActiveSection = vi.fn();
    const options = {
      checkModels: vi.fn(async () => ({ hasModels: true })),
      setActiveSection,
      setSourceFilter: vi.fn(),
      setForceShowOnboarding: vi.fn(),
      forceOnboardingNeedsFreshAvailabilityRef: { current: false },
    };
    const mounted = renderHook(() => useAppEvents(options), { wrapper: StrictMode });
    expect(pendingOverview).toHaveLength(2);

    await act(async () => pendingOverview[1]());
    await waitFor(() => expect(listeners.get("hotkey-registration-failed")?.size).toBe(1));
    await act(async () => pendingOverview[0]());
    await act(async () => {});

    expect(
      vi.mocked(listen).mock.calls.filter(([event]) => event === "hotkey-registration-failed"),
    ).toHaveLength(1);
    expect(listeners.get("hotkey-registration-failed")?.size).toBe(1);
    act(() => {
      for (const handler of listeners.get("navigate-to-overview") ?? []) {
        handler({ payload: null } as Event<unknown>);
      }
    });
    expect(setActiveSection).toHaveBeenCalledWith("home");
    mounted.unmount();
    expect(listeners.get("hotkey-registration-failed")?.size).toBe(0);
  });

  it("routes tray and island events to main while the island is active", async () => {
    const options = {
      checkModels: vi.fn(async () => ({ hasModels: true })),
      setActiveSection: vi.fn(),
      setSourceFilter: vi.fn(),
      openSettingsPane: vi.fn(),
      setForceShowOnboarding: vi.fn(),
      forceOnboardingNeedsFreshAvailabilityRef: { current: false },
    };
    const mounted = renderHook(() => useAppEvents(options));
    await act(async () => pendingOverview[0]());
    await waitFor(() => expect(listeners.get("island-navigate")?.size).toBe(1));
    eventCoordinator.setActiveWindow("pill");
    act(() => {
      for (const handler of listeners.get("main-navigate") ?? [])
        handler({ payload: { screen: "settings", pane: "shortcuts" } } as Event<unknown>);
      for (const handler of listeners.get("island-navigate") ?? [])
        handler({ payload: "open_cloud_keys" } as Event<unknown>);
    });
    expect(options.openSettingsPane).toHaveBeenCalledWith("shortcuts");
    expect(options.setSourceFilter).toHaveBeenCalledWith("cloud");
    expect(options.setActiveSection).toHaveBeenCalledWith("transcription");
    mounted.unmount();
  });

  it("routes backend Settings pane events and accepts only known section ids", async () => {
    const setActiveSection = vi.fn();
    renderHook(() =>
      useAppEvents({
        checkModels: vi.fn(async () => ({ hasModels: true })),
        setActiveSection,
        setSourceFilter: vi.fn(),
        setForceShowOnboarding: vi.fn(),
        forceOnboardingNeedsFreshAvailabilityRef: { current: false },
      }),
    );
    await act(async () => pendingOverview[0]());
    await waitFor(() => expect(listeners.get("navigate-to-section")?.size).toBe(1));
    for (const pane of [
      "general",
      "shortcuts",
      "privacy",
      "storage",
      "network",
      "agent",
      "advanced",
      "license",
      "about",
    ]) {
      act(() => {
        for (const handler of listeners.get("navigate-to-settings") ?? [])
          handler({ payload: pane } as Event<unknown>);
      });
      expect(setActiveSection).toHaveBeenLastCalledWith(pane);
    }
    act(() => {
      for (const handler of listeners.get("navigate-to-settings") ?? [])
        handler({ payload: undefined } as Event<unknown>);
    });
    expect(setActiveSection).toHaveBeenLastCalledWith("settings");
    act(() => {
      for (const handler of listeners.get("navigate-to-section") ?? [])
        handler({ payload: "network" } as Event<unknown>);
    });
    expect(setActiveSection).toHaveBeenLastCalledWith("network");
    setActiveSection.mockClear();
    act(() => {
      for (const handler of listeners.get("navigate-to-section") ?? [])
        handler({ payload: "unknown" } as Event<unknown>);
    });
    expect(setActiveSection).not.toHaveBeenCalled();
  });

  it.each(["main-navigate", "navigate-to-section", "navigate-to-settings"] as const)(
    "%s opens every Settings pane without replacing the page",
    async (eventName) => {
      const options = {
        checkModels: vi.fn(async () => ({ hasModels: true })),
        setActiveSection: vi.fn(),
        setSourceFilter: vi.fn(),
        openSettingsPane: vi.fn(),
        setForceShowOnboarding: vi.fn(),
        forceOnboardingNeedsFreshAvailabilityRef: { current: false },
      };
      const mounted = renderHook(() => useAppEvents(options));
      await act(async () => pendingOverview[0]());
      await waitFor(() => expect(listeners.get("navigate-to-section")?.size).toBe(1));
      for (const pane of [
        "general",
        "shortcuts",
        "privacy",
        "storage",
        "network",
        "agent",
        "advanced",
        "license",
        "about",
      ]) {
        act(() => {
          for (const handler of listeners.get(eventName) ?? [])
            handler({
              payload: eventName === "main-navigate" ? { screen: "settings", pane } : pane,
            } as Event<unknown>);
        });
        expect(options.openSettingsPane).toHaveBeenLastCalledWith(pane);
      }
      expect(options.setActiveSection).not.toHaveBeenCalled();
      mounted.unmount();
    },
  );

  it.each(["main-navigate", "navigate-to-section"] as const)(
    "%s opens all page destinations including Insights",
    async (eventName) => {
      const options = {
        checkModels: vi.fn(async () => ({ hasModels: true })),
        setActiveSection: vi.fn(),
        setSourceFilter: vi.fn(),
        openSettingsPane: vi.fn(),
        setForceShowOnboarding: vi.fn(),
        forceOnboardingNeedsFreshAvailabilityRef: { current: false },
      };
      const mounted = renderHook(() => useAppEvents(options));
      await act(async () => pendingOverview[0]());
      await waitFor(() => expect(listeners.get("navigate-to-section")?.size).toBe(1));
      for (const screen of [
        "home",
        "history",
        "insights",
        "models",
        "transcription",
        "polish",
        "recording",
        "dictionary",
        "help",
      ]) {
        act(() => {
          for (const handler of listeners.get(eventName) ?? [])
            handler({
              payload: eventName === "main-navigate" ? { screen } : screen,
            } as Event<unknown>);
        });
        expect(options.setActiveSection).toHaveBeenLastCalledWith(screen);
      }
      expect(options.openSettingsPane).not.toHaveBeenCalled();
      mounted.unmount();
    },
  );

  it("an older cleanup cannot remove a replacement registration", async () => {
    const first = await eventCoordinator.register("main", "test-cleanup", vi.fn());
    const secondHandler = vi.fn();
    const second = await eventCoordinator.register("main", "test-cleanup", secondHandler);
    first();
    expect(listeners.get("test-cleanup")?.size).toBe(1);
    act(() => {
      for (const handler of listeners.get("test-cleanup") ?? []) {
        handler({ payload: null } as Event<unknown>);
      }
    });
    expect(secondHandler).toHaveBeenCalledOnce();
    second();
  });
});
