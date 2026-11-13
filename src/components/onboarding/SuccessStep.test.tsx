import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { SuccessStep } from "@/components/onboarding/SuccessStep";
const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@/components/tabs/overview/useTestDictation", () => ({ useTestDictation: () => ({ feedback: null, contentChanged: vi.fn() }) }));

it("offers an explicit Stop while practicing, without toggling a new recording", async () => {
  mocks.invoke.mockImplementation(async (command: string) => command === "get_current_recording_state"
    ? { state: "recording", generation: 1, error: null } : undefined);
  render(<SuccessStep onChangeShortcut={vi.fn()} capturedBareModifier={null} holdToTalk={false}
    hotkey="Alt+Space" telemetryOptIn={false} isSavingCompletion={false} onTelemetryChange={vi.fn()} onComplete={vi.fn()} />);
  await userEvent.setup().click(await screen.findByRole("button", { name: "Stop" }));
  expect(mocks.invoke).toHaveBeenCalledWith("stop_recording");
  expect(mocks.invoke).not.toHaveBeenCalledWith("toggle_recording");
});
