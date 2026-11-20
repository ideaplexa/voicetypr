import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { capabilityGroups } from "@/components/capabilities/capabilityCatalog";
import { usePolishCanRun } from "@/lib/polish-ready";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const status = (overrides: Record<string, unknown>) => ({
  will_run: false, own_provider_ready: false, own_provider_enabled: false, builtin_enabled: true, builtin: "ready", ...overrides,
});
const answer = (value: Record<string, unknown>) =>
  invokeMock.mockImplementation((command: string) => Promise.resolve(command === "polish_status" ? value : null));
const polishBadge = (readiness: ReturnType<typeof usePolishCanRun>) =>
  capabilityGroups(false, readiness).flatMap((group) => group.items).find((item) => item.id === "polish")?.badge;
const settle = async (value: Record<string, unknown>) => {
  answer(value);
  const { result } = renderHook(() => usePolishCanRun());
  await waitFor(() => expect(result.current).not.toBeNull());
  return result.current;
};

describe("usePolishCanRun", () => {
  beforeEach(() => invokeMock.mockReset());

  it("says built-in Polish works once Voicetypr's server confirmed it, asking the server once", async () => {
    const readiness = await settle(status({ will_run: true }));
    expect(readiness).toBe("works");
    expect(polishBadge(readiness)).toBe("Works now");
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalledWith("polish_can_run");
  });

  it.each(["unreachable", "not_entitled", "no_access"])(
    "never says built-in Polish works, or needs a provider, when the server says %s",
    async (builtin) => {
      const readiness = await settle(status({ builtin }));
      expect(readiness).toBe("waiting");
      expect(polishBadge(readiness)).toBe("Not available yet");
    },
  );

  it("asks to turn on a provider of the user's own that is set up but off", async () => {
    const readiness = await settle(status({ own_provider_ready: true, builtin: "own_provider" }));
    expect(readiness).toBe("turn_on");
    expect(polishBadge(readiness)).toBe("Turn on first");
  });

  it("says a provider is needed when built-in Polish is off and none is set up", async () => {
    const readiness = await settle(status({ builtin_enabled: false }));
    expect(readiness).toBe("needs_provider");
    expect(polishBadge(readiness)).toBe("Needs a provider");
  });
});
