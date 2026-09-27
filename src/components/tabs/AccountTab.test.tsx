import { render } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { AccountTab } from "./AccountTab";

// Mock sonner
vi.mock("sonner", () => ({
  toast: {
    info: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock Tauri
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd) => {
    if (cmd === "focus_main_window") return Promise.resolve();
    return Promise.reject(new Error(`Unknown command: ${cmd}`));
  }),
}));

// Mock contexts
vi.mock("@/contexts/LicenseContext", () => ({
  useLicense: () => ({
    licenseStatus: {
      is_licensed: true,
      license_key: "TEST-KEY-123",
      email: "test@example.com",
    },
    checkLicense: vi.fn(),
  }),
}));

// Mock hooks
vi.mock("@/hooks/useEventCoordinator", () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn((event: string, callback: any) => {
      (window as any).__testEventCallbacks = (window as any).__testEventCallbacks || {};
      (window as any).__testEventCallbacks[event] = callback;
      return vi.fn();
    }),
  }),
}));

// Mock AccountSection component
vi.mock("@/components/sections/AccountSection", () => ({
  AccountSection: () => (
    <div data-testid="account-section">
      <div>Licensed: Yes</div>
      <div>Email: test@example.com</div>
    </div>
  ),
}));

describe("AccountTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it("shows error toast for license-required event", async () => {
    vi.useFakeTimers();
    const { toast } = await import("sonner");
    render(<AccountTab />);

    const callback = (window as any).__testEventCallbacks["license-required"];
    callback({ message: "License required for AI Enhancement" });

    // Wait for the timeouts in the component
    await vi.advanceTimersByTimeAsync(300);

    expect(toast.error).toHaveBeenCalledWith(
      "License required for AI Enhancement",
      expect.any(Object),
    );

    vi.useRealTimers();
  });
});
