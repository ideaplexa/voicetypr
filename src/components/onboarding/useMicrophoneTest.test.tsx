import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useMicrophoneTest } from "@/components/onboarding/useMicrophoneTest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.handlers.set(name, handler);
    return () => { if (mocks.handlers.get(name) === handler) mocks.handlers.delete(name); };
  }),
}));

beforeEach(() => {
  vi.useFakeTimers();
  mocks.handlers.clear();
  mocks.invoke.mockReset();
  mocks.invoke.mockImplementation(async (name: string) => name === "start_microphone_test" ? "test-1" : undefined);
});
afterEach(() => vi.useRealTimers());

it("shows actual input and releases capture when leaving the mic step", async () => {
  const { result, unmount } = renderHook(() => useMicrophoneTest("Selected input"));
  await act(async () => {});
  expect(mocks.invoke).toHaveBeenCalledWith("start_microphone_test", { deviceName: "Selected input" });
  act(() => mocks.handlers.get("microphone-test-level")?.({ payload: { session_id: "test-1", level: 0.04 } }));
  expect(result.current.status).toBe("heard");
  expect(result.current.level).toBeCloseTo(0.32);
  unmount();
  expect(mocks.invoke).toHaveBeenCalledWith("stop_microphone_test", { sessionId: "test-1" });
});

it("treats silence as no input yet, not a microphone failure", async () => {
  const { result } = renderHook(() => useMicrophoneTest(undefined));
  await act(async () => {});
  act(() => vi.advanceTimersByTime(4000));
  expect(result.current.status).toBe("quiet");
  act(() => mocks.handlers.get("microphone-test-level")?.({ payload: { session_id: "old-test", level: 1 } }));
  expect(result.current.status).toBe("quiet");
  act(() => mocks.handlers.get("microphone-test-level")?.({ payload: { session_id: "test-1", level: 0.1 } }));
  expect(result.current.status).toBe("heard");
});

it("stops a late capture start after the mic step has unmounted", async () => {
  let resolveStart: (id: string) => void = () => {};
  const pending = new Promise<string>((resolve) => { resolveStart = resolve; });
  mocks.invoke.mockImplementation((name: string) => name === "start_microphone_test" ? pending : Promise.resolve());
  const { unmount } = renderHook(() => useMicrophoneTest(undefined));
  await act(async () => {});
  unmount();
  await act(async () => { resolveStart("late-test"); await pending; });
  expect(mocks.invoke).toHaveBeenCalledWith("stop_microphone_test", { sessionId: "late-test" });
});
