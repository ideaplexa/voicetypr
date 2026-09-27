import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PrivacyConsentDialog } from "./PrivacyConsentDialog";

const mockInvoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("sonner", () => ({
  toast: { error: vi.fn() },
}));

describe("PrivacyConsentDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: true, available: true });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({
          enabled: true,
          available: true,
          consent_required: true,
        });
      }
      return Promise.resolve(undefined);
    });
  });

  it("requires one acknowledgement and persists both independent choices", async () => {
    render(<PrivacyConsentDialog />);

    expect(await screen.findByText("Help improve Voicetypr")).toBeInTheDocument();
    const analytics = screen.getByRole("switch", {
      name: "Enable usage analytics",
    });
    fireEvent.click(analytics);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_telemetry_consent", {
        enabled: true,
      });
      expect(mockInvoke).toHaveBeenCalledWith("set_product_analytics_consent", {
        enabled: false,
      });
    });
    expect(
      mockInvoke.mock.calls
        .map(([command]) => command)
        .filter((command) => command.startsWith("set_")),
    ).toEqual(["set_telemetry_consent", "set_product_analytics_consent"]);
    await waitFor(() =>
      expect(screen.queryByText("Help improve Voicetypr")).not.toBeInTheDocument(),
    );
  });

  it("keeps the prompt pending after a partial consent save", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: true, available: true });
      }
      if (command === "get_product_analytics_status") {
        return Promise.resolve({
          enabled: true,
          available: true,
          consent_required: true,
        });
      }
      if (command === "set_product_analytics_consent") {
        return Promise.reject(new Error("store unavailable"));
      }
      return Promise.resolve(undefined);
    });
    render(<PrivacyConsentDialog />);

    await screen.findByText("Help improve Voicetypr");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("set_product_analytics_consent", {
        enabled: true,
      });
    });
    expect(screen.getByText("Help improve Voicetypr")).toBeInTheDocument();
  });

  it("pauses unacknowledged analytics for the session when deferred", async () => {
    render(<PrivacyConsentDialog />);

    await screen.findByText("Help improve Voicetypr");
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("defer_privacy_consent_for_session");
    });
  });

  it("stays closed when persisted opt-outs are read on launch", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_telemetry_status") {
        return Promise.resolve({ enabled: false, available: true });
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

    render(<PrivacyConsentDialog />);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_product_analytics_status");
    });
    expect(screen.queryByText("Help improve Voicetypr")).not.toBeInTheDocument();
  });
});
