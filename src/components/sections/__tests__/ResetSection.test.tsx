import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";
import { ResetSection } from "@/components/sections/ResetSection";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ updateSettings: vi.fn() }),
}));
vi.mock("@/lib/logger", () => ({
  createLogger: () => ({ error: vi.fn() }),
}));
vi.mock("sonner", () => ({
  toast: { error: vi.fn(), success: vi.fn() },
}));

async function resetApp() {
  render(<ResetSection />);
  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Reset App Data" }));
  });
}

describe("ResetSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(relaunch).mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("explains that license and API keys survive reset", async () => {
    vi.mocked(ask).mockResolvedValue(false);
    await resetApp();
    expect(screen.getByText("Keep your license and API keys")).toBeInTheDocument();
    expect(screen.getByText("Delete all transcription history")).toBeInTheDocument();
    expect(screen.getByText("Remove all downloaded models")).toBeInTheDocument();
    expect(ask).toHaveBeenCalledWith(
      "This action cannot be undone. This will permanently delete all your Voicetypr data except your license and API keys.\n\nThe app will restart after reset.\n\nAre you absolutely sure?",
      expect.objectContaining({ title: "Reset App Data", kind: "warning" }),
    );
    expect(invoke).not.toHaveBeenCalled();
  });

  it("relaunches only after a successful reset", async () => {
    vi.mocked(invoke).mockResolvedValue({
      success: true,
      errors: [],
      cleared_items: ["Settings store"],
    });

    await resetApp();
    expect(invoke).toHaveBeenCalledWith("reset_app_data");
    expect(toast.success).toHaveBeenCalledWith("App data reset successfully. Restarting...");
    expect(relaunch).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(relaunch).toHaveBeenCalledOnce();
  });

  it("shows incomplete reset errors without restarting and allows retry", async () => {
    vi.mocked(invoke).mockResolvedValue({
      success: false,
      errors: ["Could not clear cache", "Could not clear models"],
      cleared_items: ["Settings store"],
    });

    await resetApp();
    expect(toast.error).toHaveBeenCalledWith(
      "Reset could not remove everything. Please try again.",
    );
    expect(screen.getByRole("button", { name: "Reset App Data" })).toBeEnabled();
    await act(async () => {
      await vi.runAllTimersAsync();
    });
    expect(toast.success).not.toHaveBeenCalled();
    expect(relaunch).not.toHaveBeenCalled();
  });

  it("shows a fallback message when an incomplete reset has no error details", async () => {
    vi.mocked(invoke).mockResolvedValue({
      success: false,
      errors: [],
      cleared_items: [],
    });

    await resetApp();
    expect(toast.error).toHaveBeenCalledWith(
      "Reset could not remove everything. Please try again.",
    );
    await act(async () => {
      await vi.runAllTimersAsync();
    });
    expect(relaunch).not.toHaveBeenCalled();
  });

  it("does not restart when the reset command rejects unsafe storage", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("Refusing reset outside app-owned storage"));

    await resetApp();
    expect(toast.error).toHaveBeenCalledWith("Failed to reset app data");
    expect(screen.getByRole("button", { name: "Reset App Data" })).toBeEnabled();
    await act(async () => {
      await vi.runAllTimersAsync();
    });
    expect(relaunch).not.toHaveBeenCalled();
  });

  it("does nothing when the confirmation is cancelled", async () => {
    vi.mocked(ask).mockResolvedValue(false);
    await resetApp();
    expect(invoke).not.toHaveBeenCalled();
    expect(toast.success).not.toHaveBeenCalled();
    expect(relaunch).not.toHaveBeenCalled();
  });
});
