import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { capabilityGroups } from "@/components/capabilities/capabilityCatalog";
import { usePolishCanRun } from "@/lib/polish-ready";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const answer = (canRun: boolean, status: Record<string, unknown>) =>
  invokeMock.mockImplementation((command: string) =>
    Promise.resolve(command === "polish_can_run" ? canRun : command === "polish_status" ? status : null));

const polishBadge = (readiness: ReturnType<typeof usePolishCanRun>) =>
  capabilityGroups(false, readiness).flatMap((group) => group.items).find((item) => item.id === "polish")?.badge;

describe("usePolishCanRun", () => {
  beforeEach(() => invokeMock.mockReset());

  it("says Polish works when the gate dictation uses says it will run, built-in Polish included", async () => {
    answer(true, {});
    const { result } = renderHook(() => usePolishCanRun());
    await waitFor(() => expect(result.current).toBe("works"));
    expect(polishBadge(result.current)).toBe("Works now");
  });

  it("never says built-in Polish works, or needs a provider, before the server confirmed it", async () => {
    answer(false, { will_run: false, own_provider_ready: false, own_provider_enabled: false, builtin_enabled: true, builtin: "unreachable" });
    const { result } = renderHook(() => usePolishCanRun());
    await waitFor(() => expect(result.current).toBe("waiting"));
    expect(polishBadge(result.current)).toBe("Not available yet");
  });

  it("says a provider is needed when built-in Polish is off and none is set up", async () => {
    answer(false, { will_run: false, own_provider_ready: false, own_provider_enabled: false, builtin_enabled: false, builtin: "ready" });
    const { result } = renderHook(() => usePolishCanRun());
    await waitFor(() => expect(result.current).toBe("needs_provider"));
    expect(polishBadge(result.current)).toBe("Needs a provider");
  });
});
