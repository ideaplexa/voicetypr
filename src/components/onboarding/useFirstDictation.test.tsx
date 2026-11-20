import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const handlers = vi.hoisted(() => new Map<string, (event: { payload: unknown }) => void>());
const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
    handlers.set(event, handler);
    return Promise.resolve(() => handlers.delete(event));
  }),
}));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

import { useFirstDictation } from "./useFirstDictation";

const cancels = () => mockInvoke.mock.calls.filter(([command]) => command === "cancel_onboarding_try");
const escape = () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape" }));

async function startTry() {
  const onResult = vi.fn();
  const hook = renderHook(() => useFirstDictation(true, false, onResult));
  await act(async () => {});
  await act(async () => handlers.get("onboarding-try-key")?.({ payload: true }));
  return { hook, onResult };
}

describe("useFirstDictation Esc Esc", () => {
  beforeEach(() => {
    handlers.clear();
    mockInvoke.mockReset();
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === "get_onboarding_try") return { kind: "tries", left: 9 };
      if (command === "start_onboarding_try") return { id: "00000000000000aa", left: 8 };
      if (command === "stop_onboarding_try") return { heard: "hello there" };
      return undefined;
    });
  });

  it("throws the try away on a second Esc and shows no result", async () => {
    const { hook, onResult } = await startTry();
    expect(hook.result.current.listening).toBe(true);

    act(() => escape());
    expect(hook.result.current.listening).toBe(true);
    act(() => escape());

    expect(hook.result.current.listening).toBe(false);
    expect(mockInvoke).toHaveBeenCalledWith("cancel_onboarding_try", { id: "00000000000000aa" });
    await act(async () => handlers.get("onboarding-try-key")?.({ payload: true }));
    expect(onResult).not.toHaveBeenCalled();
  });

  it("cancels a try still starting once its start returns, never before", async () => {
    let started: (value: { id: string; left: number }) => void = () => {};
    mockInvoke.mockImplementation(async (command: string) => {
      if (command === "get_onboarding_try") return { kind: "tries", left: 9 };
      if (command === "start_onboarding_try") return new Promise((resolve) => (started = resolve));
      return undefined;
    });
    const hook = renderHook(() => useFirstDictation(true, false, vi.fn()));
    await act(async () => {});
    await act(async () => handlers.get("onboarding-try-key")?.({ payload: true }));

    act(() => {
      escape();
      escape();
    });
    expect(cancels()).toHaveLength(0);

    await act(async () => started({ id: "00000000000000aa", left: 8 }));
    expect(mockInvoke).toHaveBeenCalledWith("cancel_onboarding_try", { id: "00000000000000aa" });
    expect(hook.result.current.listening).toBe(false);
  });

  it("ignores Esc when no try is running", async () => {
    renderHook(() => useFirstDictation(true, false, vi.fn()));
    await act(async () => {});
    act(() => {
      escape();
      escape();
    });
    expect(cancels()).toHaveLength(0);
  });
});
