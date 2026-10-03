import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { PrivacyConsentDialog } from "./PrivacyConsentDialog";
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));
beforeEach(() => {
  mockInvoke.mockReset().mockImplementation(async (command: string) => command === "get_telemetry_status"
    ? { enabled: true, available: true, consent_required: true } : undefined);
});
it("shows one pre-ticked control and saves one unified opt-out", async () => {
  render(<PrivacyConsentDialog />);
  await screen.findByText("Help improve Voicetypr");
  const control = screen.getByRole("switch");
  expect(control).toBeChecked();
  fireEvent.click(control);
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("set_telemetry_consent", { enabled: false }));
  expect(mockInvoke).not.toHaveBeenCalledWith("set_product_analytics_consent", expect.anything());
});
it("keeps the prompt pending after a save failure", async () => {
  mockInvoke.mockImplementation(async (command: string) => {
    if (command === "get_telemetry_status") return { enabled: true, consent_required: true };
    throw new Error("save failed");
  });
  render(<PrivacyConsentDialog />);
  await screen.findByText("Help improve Voicetypr");
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("set_telemetry_consent", { enabled: true }));
  expect(screen.getByText("Help improve Voicetypr")).toBeInTheDocument();
});
it("pauses all sharing when deferred", async () => {
  render(<PrivacyConsentDialog />);
  await screen.findByText("Help improve Voicetypr");
  fireEvent.click(screen.getByRole("button", { name: "Not now" }));
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("defer_privacy_consent_for_session"));
});
it("stays closed after acknowledgement", async () => {
  mockInvoke.mockResolvedValue({ enabled: false, available: true, consent_required: false });
  render(<PrivacyConsentDialog />);
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_telemetry_status"));
  expect(screen.queryByText("Help improve Voicetypr")).not.toBeInTheDocument();
});
it("explains categories in the shared-information dialog", async () => {
  render(<PrivacyConsentDialog />);
  await screen.findByText("Help improve Voicetypr");
  fireEvent.click(screen.getByRole("button", { name: "What's shared" }));
  expect(await screen.findByText(/Random installation and dictation IDs/)).toBeInTheDocument();
});
