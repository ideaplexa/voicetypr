import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";

vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: { selected_microphone: "" } }) }));
vi.mock("@/components/onboarding/useMicrophoneTest", () => ({ useMicrophoneTest: () => ({ level: 0, status: "opening", visible: true }) }));

it("names the initial Accessibility registration action and explains the settings handoff", async () => {
  const request = vi.fn();
  render(<PermissionsStep permissions={{ microphone: { status: "granted" }, accessibility: { status: "not_determined" } }}
    isRequestingPermission={null} onRequest={request} />);
  expect(screen.getByText("So your words can be typed into any app.")).toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole("button", { name: "Enable Accessibility" }));
  expect(request).toHaveBeenCalledWith("accessibility");
});
