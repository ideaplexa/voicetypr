import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";

vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: { selected_microphone: "" } }) }));
vi.mock("@/components/onboarding/useMicrophoneTest", () => ({ useMicrophoneTest: () => ({ level: 0, status: "opening", visible: true }) }));
import type { PermissionStatus } from "@/components/onboarding/onboardingTypes";

it.each<[PermissionStatus, string]>([
  ["not_determined", "Allow microphone"],
  ["denied", "Open System Settings"],
  ["restricted", "Open System Settings"],
])("names the actual microphone action for %s", async (status, label) => {
  const request = vi.fn();
  render(<PermissionsStep permissions={{ microphone: { status }, accessibility: { status: "granted" } }}
    isRequestingPermission={null} onRequest={request} />);
  await userEvent.setup().click(screen.getByRole("button", { name: label }));
  expect(request).toHaveBeenCalledWith("microphone");
});

it("directs Accessibility to the settings where it must be enabled", async () => {
  const request = vi.fn();
  render(<PermissionsStep permissions={{ microphone: { status: "granted" }, accessibility: { status: "denied" } }}
    isRequestingPermission={null} onRequest={request} />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Open System Settings" }));
  expect(request).toHaveBeenCalledWith("accessibility");
});
