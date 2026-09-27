import { describe, it, expect, vi } from "vitest";
import { renderHook } from "@testing-library/react";
import { useLicenseStatus } from "./useLicenseStatus";

const mockUseLicense = vi.fn();

vi.mock("@/contexts/LicenseContext", () => ({
  useLicense: () => mockUseLicense(),
}));

describe("useLicenseStatus", () => {
  it("treats licensed status as valid", () => {
    mockUseLicense.mockReturnValue({
      status: { status: "licensed" },
      isLoading: false,
      checkStatus: vi.fn(),
    });

    const { result } = renderHook(() => useLicenseStatus());

    expect(result.current.isValid).toBe(true);
    expect(result.current.isChecking).toBe(false);
  });

  it("treats expired status as invalid", () => {
    mockUseLicense.mockReturnValue({
      status: { status: "expired" },
      isLoading: false,
      checkStatus: vi.fn(),
    });

    const { result } = renderHook(() => useLicenseStatus());

    expect(result.current.isValid).toBe(false);
  });
});
