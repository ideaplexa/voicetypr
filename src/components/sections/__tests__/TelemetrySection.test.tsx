import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TelemetrySection } from "../TelemetrySection";
import { toast } from "sonner";

const mockInvoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("sonner", () => ({
  toast: {
    info: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

describe("TelemetrySection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: true, available: true });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({
          enabled: false,
          available: true,
          consent_required: false,
        });
      }
      return Promise.resolve(undefined);
    });
  });

  it("renders independent crash and usage analytics controls", async () => {
    render(<TelemetrySection />);

    const diagnostics = await screen.findByRole("switch", {
      name: "Enable crash and error reporting",
    });
    const analytics = screen.getByRole("switch", {
      name: "Enable usage analytics",
    });

    expect(diagnostics).toBeChecked();
    expect(analytics).not.toBeChecked();
    expect(screen.getByText("Crash & error reporting")).toBeInTheDocument();
    expect(screen.getByText("Usage analytics")).toBeInTheDocument();
  });

  it("updates product analytics without changing crash consent", async () => {
    render(<TelemetrySection />);

    const analytics = await screen.findByRole("switch", {
      name: "Enable usage analytics",
    });
    await act(async () => {
      fireEvent.click(analytics);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_product_analytics_consent", {
        enabled: true,
      });
    });
    expect(mockInvoke).not.toHaveBeenCalledWith("set_telemetry_consent", expect.anything());
  });

  it("allows opting out of an unavailable category", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: true, available: false });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({
          enabled: true,
          available: false,
          consent_required: false,
        });
      }
      if (command === "set_telemetry_consent") {
        return Promise.resolve({ enabled: false, restart_required: false });
      }
      return Promise.resolve(undefined);
    });

    render(<TelemetrySection />);

    const diagnostics = await screen.findByRole("switch", {
      name: "Enable crash and error reporting",
    });
    expect(diagnostics).toBeEnabled();

    await act(async () => {
      fireEvent.click(diagnostics);
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_telemetry_consent", {
        enabled: false,
      });
      // Opt-out is live immediately even while reporting is unavailable:
      // the switch flips off and no restart is mentioned.
      const successMessages = vi.mocked(toast.success).mock.calls.map((call) => String(call[0]));
      expect(successMessages.length).toBeGreaterThan(0);
      expect(successMessages.some((message) => /restart/i.test(message))).toBe(false);
      expect(
        screen.getByRole("switch", { name: "Enable crash and error reporting" }),
      ).not.toBeChecked();
    });
  });

  it("says a restart is needed when enabling crash reporting mid-session", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: false, available: true });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({ enabled: false, available: true, consent_required: false });
      }
      if (command === "set_telemetry_consent") {
        return Promise.resolve({ enabled: true, restart_required: true });
      }
      return Promise.resolve(undefined);
    });

    render(<TelemetrySection />);

    const diagnostics = await screen.findByRole("switch", {
      name: "Enable crash and error reporting",
    });
    await act(async () => {
      fireEvent.click(diagnostics);
    });

    // The success guidance must tell the user a restart is needed; the
    // switch reflects the stored consent.
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_telemetry_consent", { enabled: true });
      expect(toast.success).toHaveBeenCalledWith(expect.stringContaining("restart"));
      expect(
        screen.getByRole("switch", { name: "Enable crash and error reporting" }),
      ).toBeChecked();
    });
  });

  it("keeps opting out of crash reporting immediate", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: true, available: true });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({ enabled: false, available: true, consent_required: false });
      }
      if (command === "set_telemetry_consent") {
        return Promise.resolve({ enabled: false, restart_required: false });
      }
      return Promise.resolve(undefined);
    });

    render(<TelemetrySection />);

    const diagnostics = await screen.findByRole("switch", {
      name: "Enable crash and error reporting",
    });
    await act(async () => {
      fireEvent.click(diagnostics);
    });

    // Opt-out is live immediately: the on-state flips off and no restart is
    // mentioned.
    await waitFor(() => {
      const successMessages = vi.mocked(toast.success).mock.calls.map((call) => String(call[0]));
      expect(successMessages.length).toBeGreaterThan(0);
      expect(successMessages.some((message) => /restart/i.test(message))).toBe(false);
      expect(
        screen.getByRole("switch", { name: "Enable crash and error reporting" }),
      ).not.toBeChecked();
    });
  });

  it("follows the typed consent result instead of assuming a restart", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: false, available: true });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({ enabled: false, available: true, consent_required: false });
      }
      if (command === "set_telemetry_consent") {
        return Promise.resolve({ enabled: true, restart_required: false });
      }
      return Promise.resolve(undefined);
    });

    render(<TelemetrySection />);

    const diagnostics = await screen.findByRole("switch", {
      name: "Enable crash and error reporting",
    });
    await act(async () => {
      fireEvent.click(diagnostics);
    });

    // No restart guidance when the typed result says reporting is live; the
    // switch still reflects the stored consent.
    await waitFor(() => {
      const successMessages = vi.mocked(toast.success).mock.calls.map((call) => String(call[0]));
      expect(successMessages.length).toBeGreaterThan(0);
      expect(successMessages.some((message) => /restart/i.test(message))).toBe(false);
      expect(
        screen.getByRole("switch", { name: "Enable crash and error reporting" }),
      ).toBeChecked();
    });
  });
});
