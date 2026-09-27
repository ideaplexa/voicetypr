import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AccountSection } from "../AccountSection";

const mockUseLicense = vi.fn();
const revalidateLicense = vi.fn();

vi.mock("@/contexts/LicenseContext", () => ({
  useLicense: () => mockUseLicense(),
}));

vi.mock("@tauri-apps/plugin-shell", () => ({
  open: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: { error: vi.fn() },
}));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: { onboarding_completed: true },
    updateSettings: vi.fn().mockResolvedValue(undefined),
  }),
}));

describe("AccountSection license verification", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUseLicense.mockReturnValue({
      status: {
        status: "licensed",
        license_type: "pro",
        license_key: "VT-TEST-LICENSE-1234",
        expires_at: "89 days offline remaining",
        verification_state: "needs_revalidation",
      },
      isLoading: false,
      checkStatus: vi.fn(),
      revalidateLicense,
      activateLicense: vi.fn(),
      deactivateLicense: vi.fn(),
      openPurchasePage: vi.fn(),
    });
  });

  it("keeps Pro active and offers revalidation instead of showing Trial Expired", () => {
    render(<AccountSection />);

    expect(screen.getByText("Pro Licensed")).toBeInTheDocument();
    expect(screen.getByText("License verification still unavailable")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "License" })).toBeInTheDocument();
    expect(screen.queryByText("Reset app / start over")).not.toBeInTheDocument();
    expect(
      screen.getByText("Offline access remains available. Your paid license has not expired."),
    ).toBeInTheDocument();
    expect(screen.queryByText("Trial Expired")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Revalidate now" }));
    expect(revalidateLicense).toHaveBeenCalledTimes(1);
  });

  it("keeps revalidation reachable after a verified runtime status reaches its deadline", () => {
    mockUseLicense.mockReturnValue({
      status: {
        status: "licensed",
        license_type: "pro",
        license_key: "VT-TEST-LICENSE-1234",
        verification_state: "verified",
        verification_expires_at: "2026-07-24T00:00:00Z",
      },
      isLoading: false,
      checkStatus: vi.fn(),
      revalidateLicense,
      activateLicense: vi.fn(),
      deactivateLicense: vi.fn(),
      openPurchasePage: vi.fn(),
    });

    render(<AccountSection />);

    fireEvent.click(screen.getByRole("button", { name: "Revalidate License" }));
    expect(revalidateLicense).toHaveBeenCalledTimes(1);
  });
});
