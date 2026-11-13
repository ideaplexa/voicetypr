import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";
import type { PermissionStatus } from "@/components/onboarding/onboardingTypes";

it.each<[PermissionStatus, string]>([
  ["not_determined", "Allow microphone"],
  ["denied", "Open System Settings"],
  ["restricted", "Open System Settings"],
])("names the actual microphone action for %s", async (status, label) => {
  const request = vi.fn();
  render(<PermissionsStep permissions={{ microphone: { status }, accessibility: { status: "granted" } }}
    checkingPermissions={new Set()} isRequestingPermission={null} onCheck={vi.fn()} onRequest={request} />);
  await userEvent.setup().click(screen.getByRole("button", { name: label }));
  expect(request).toHaveBeenCalledWith("microphone");
});

it("directs Accessibility to the settings where it must be enabled", async () => {
  const request = vi.fn();
  render(<PermissionsStep permissions={{ microphone: { status: "granted" }, accessibility: { status: "denied" } }}
    checkingPermissions={new Set()} isRequestingPermission={null} onCheck={vi.fn()} onRequest={request} />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Open System Settings" }));
  expect(request).toHaveBeenCalledWith("accessibility");
});
