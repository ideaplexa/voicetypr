import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";

it("names the initial Accessibility registration action and explains the settings handoff", async () => {
  const request = vi.fn();
  render(<PermissionsStep permissions={{ microphone: { status: "granted" }, accessibility: { status: "not_determined" } }}
    checkingPermissions={new Set()} isRequestingPermission={null} onCheck={vi.fn()} onRequest={request} />);
  expect(screen.getByText("Switch on Voicetypr in the list, then come back.")).toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole("button", { name: "Enable Accessibility" }));
  expect(request).toHaveBeenCalledWith("accessibility");
});
