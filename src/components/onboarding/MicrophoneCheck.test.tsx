import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { MicrophoneCheck } from "@/components/onboarding/MicrophoneCheck";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  selected: "First input",
}));
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: { selected_microphone: mocks.selected }, refreshSettings: vi.fn() }) }));
vi.mock("@/components/MicrophoneSelection", () => ({ MicrophoneSelection: () => <button>Choose microphone</button> }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.handlers.set(name, handler);
    return () => { if (mocks.handlers.get(name) === handler) mocks.handlers.delete(name); };
  }),
}));
beforeEach(() => {
  mocks.selected = "First input";
  mocks.handlers.clear();
  mocks.invoke.mockReset();
  let session = 0;
  mocks.invoke.mockImplementation(async (command: string) => command === "start_microphone_test" ? String(++session) : undefined);
});

it("shows real proof of input and starts a fresh check for a changed microphone", async () => {
  const view = render(<MicrophoneCheck />);
  await act(async () => {});
  expect(screen.getByRole("button", { name: "Choose microphone" })).toBeInTheDocument();
  expect(screen.getByRole("status")).not.toHaveTextContent("We can hear you");
  act(() => mocks.handlers.get("microphone-test-level")?.({ payload: { session_id: "1", level: 0.05 } }));
  expect(screen.getByText("We can hear you")).toBeInTheDocument();
  expect(screen.getByRole("meter")).toHaveAttribute("aria-valuenow", "40");
  mocks.selected = "Second input";
  view.rerender(<MicrophoneCheck />);
  await act(async () => {});
  expect(mocks.invoke).toHaveBeenCalledWith("stop_microphone_test", { sessionId: "1" });
  expect(mocks.invoke).toHaveBeenCalledWith("start_microphone_test", { deviceName: "Second input" });
  expect(screen.queryByText("We can hear you")).not.toBeInTheDocument();
  view.unmount();
  expect(mocks.invoke).toHaveBeenCalledWith("stop_microphone_test", { sessionId: "2" });
});
