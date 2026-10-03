import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { TelemetrySection } from "../TelemetrySection";
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => mockInvoke(...args) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
beforeEach(() => { mockInvoke.mockReset().mockImplementation(async (command: string, args?: { enabled: boolean }) =>
  command === "get_telemetry_status" ? { enabled: true, available: true } : { enabled: args?.enabled, available: true }); });
it("renders one matching privacy switch", async () => {
  render(<TelemetrySection />);
  await waitFor(() => expect(screen.getByRole("switch")).toBeEnabled());
  expect(screen.getAllByRole("switch")).toHaveLength(1);
  expect(screen.getByRole("switch")).toBeChecked();
});
it("persists one unified opt-out", async () => {
  render(<TelemetrySection />);
  await waitFor(() => expect(screen.getByRole("switch")).toBeEnabled());
  fireEvent.click(screen.getByRole("switch"));
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("set_telemetry_consent", { enabled: false }));
});
it("permits opting out in debug builds", async () => {
  mockInvoke.mockResolvedValue({ enabled: true, available: false });
  render(<TelemetrySection />);
  await waitFor(() => expect(screen.getByRole("switch")).toBeEnabled());
});
it("reflects saved consent immediately when enabled", async () => {
  mockInvoke.mockImplementation(async (command: string) => ({ enabled: command !== "get_telemetry_status", available: true }));
  render(<TelemetrySection />);
  await waitFor(() => expect(screen.getByRole("switch")).toBeEnabled());
  expect(screen.getByRole("switch")).not.toBeChecked();
  fireEvent.click(screen.getByRole("switch"));
  await waitFor(() => expect(screen.getByRole("switch")).toBeChecked());
});
it("reflects saved opt-out immediately", async () => {
  render(<TelemetrySection />);
  await waitFor(() => expect(screen.getByRole("switch")).toBeEnabled());
  fireEvent.click(screen.getByRole("switch"));
  await waitFor(() => expect(screen.getByRole("switch")).not.toBeChecked());
});
it("retains the old choice when persistence fails", async () => {
  mockInvoke.mockImplementation(async (command: string) => {
    if (command === "get_telemetry_status") return { enabled: true, available: true };
    throw new Error("storage unavailable");
  });
  render(<TelemetrySection />);
  await waitFor(() => expect(screen.getByRole("switch")).toBeEnabled());
  fireEvent.click(screen.getByRole("switch"));
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("set_telemetry_consent", { enabled: false }));
  expect(screen.getByRole("switch")).toBeChecked();
});
