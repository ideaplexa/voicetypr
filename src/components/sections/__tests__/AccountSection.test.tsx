import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ask } from "@tauri-apps/plugin-dialog";
import { open } from "@tauri-apps/plugin-shell";
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
  it("activates a trimmed license key and retains purchase access", async () => {
    const activateLicense = vi.fn().mockResolvedValue(undefined);
    const openPurchasePage = vi.fn();
    mockUseLicense.mockReturnValue({
      ...mockUseLicense(),
      status: { status: "trial", trial_days_left: 2 },
      activateLicense,
      openPurchasePage,
    });
    render(<AccountSection />);
    fireEvent.change(screen.getByRole("textbox", { name: "License key" }), {
      target: { value: "  VT-KEY  " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Activate" }));
    await waitFor(() => expect(activateLicense).toHaveBeenCalledWith("VT-KEY"));
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "License key" })).toHaveValue(""),
    );
    fireEvent.click(screen.getByRole("button", { name: "Buy License" }));
    expect(openPurchasePage).toHaveBeenCalledTimes(1);
  });

  it("opens the existing license management portal", async () => {
    render(<AccountSection />);
    fireEvent.click(screen.getByRole("button", { name: "Manage License" }));
    await waitFor(() => expect(open).toHaveBeenCalledWith("https://polar.sh/ideaplexa/portal"));
  });

  it("deactivates only after the existing confirmation", async () => {
    const deactivateLicense = vi.fn().mockResolvedValue(undefined);
    mockUseLicense.mockReturnValue({ ...mockUseLicense(), deactivateLicense });
    vi.mocked(ask).mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    render(<AccountSection />);
    fireEvent.click(screen.getByRole("button", { name: "Deactivate License" }));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    expect(deactivateLicense).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Deactivate License" }));
    await waitFor(() => expect(deactivateLicense).toHaveBeenCalledTimes(1));
    expect(ask).toHaveBeenLastCalledWith(
      "Deactivating your license will make the app unusable.",
      expect.objectContaining({ title: "Deactivate License" }),
    );
  });

  it("shows known plan and expiry and retries status collection", () => {
    const checkStatus = vi.fn();
    const view = render(<AccountSection />);
    expect(screen.getByText("pro", { exact: true })).toBeInTheDocument();
    expect(screen.getByText("89 days offline remaining")).toBeInTheDocument();
    mockUseLicense.mockReturnValue({ ...mockUseLicense(), status: null, checkStatus });
    view.rerender(<AccountSection />);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(checkStatus).toHaveBeenCalledTimes(1);
  });
});
