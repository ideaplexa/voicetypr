import { render } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { EnhancementsTab } from "./EnhancementsTab";

// Mock sonner
vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

// Mock keyring
vi.mock("@/utils/keyring", () => ({
  getApiKey: vi.fn().mockResolvedValue(null),
}));

// Mock contexts
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: {
      ai_provider: "",
      ai_enhancement_enabled: false,
    },
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

// Mock EnhancementsSection component
vi.mock("@/components/sections/EnhancementsSection", () => ({
  EnhancementsSection: () => (
    <div data-testid="enhancements-section">
      <button>Save API Key</button>
    </div>
  ),
}));

describe("EnhancementsTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
  });

  it("shows error toast for AI authentication errors", async () => {
    const { toast } = await import("sonner");
    render(<EnhancementsTab />);

    const callback = (window as any).__testEventCallbacks["ai-enhancement-auth-error"];
    callback("Invalid API key");

    expect(toast.error).toHaveBeenCalledWith(
      "Invalid API key",
      expect.objectContaining({
        description: expect.stringContaining("API key"),
      }),
    );
  });

  it("shows warning toast for AI enhancement errors", async () => {
    const { toast } = await import("sonner");
    render(<EnhancementsTab />);

    const callback = (window as any).__testEventCallbacks["ai-enhancement-error"];
    callback("Rate limit exceeded");

    expect(toast.warning).toHaveBeenCalledWith("Rate limit exceeded");
  });
});
