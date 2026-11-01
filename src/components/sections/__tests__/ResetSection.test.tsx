import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ask } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";
import { ResetSection } from "@/components/sections/ResetSection";
import type { ResetResult } from "@/types";

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ updateSettings: vi.fn() }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

describe("ResetSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(ask).mockResolvedValue(false);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe.each([false, true])("pane=%s", (pane) => {
    const resetButton = () =>
      screen.getByRole("button", { name: pane ? "Reset…" : "Reset App Data" });

    it("reports partial failure and allows retry without restarting", async () => {
      vi.useFakeTimers();
      vi.mocked(ask).mockResolvedValue(true);
      const result: ResetResult = {
        success: false,
        errors: ["Could not clear audio storage", "Failed to clear cache"],
        cleared_items: ["Settings store"],
      };
      vi.mocked(invoke).mockResolvedValue(result);
      render(<ResetSection pane={pane} />);

      await act(async () => {
        fireEvent.click(resetButton());
      });

      expect(invoke).toHaveBeenCalledWith("reset_app_data");
      expect(toast.error).toHaveBeenCalledWith(result.errors.join("\n"));
      expect(toast.success).not.toHaveBeenCalled();
      expect(resetButton()).toBeEnabled();
      await act(() => vi.advanceTimersByTime(1000));
      expect(relaunch).not.toHaveBeenCalled();
    });

    it("reports success and restarts only after a complete reset", async () => {
      vi.useFakeTimers();
      vi.mocked(ask).mockResolvedValue(true);
      const result: ResetResult = {
        success: true,
        errors: [],
        cleared_items: ["Settings store", "Saved and temporary audio"],
      };
      vi.mocked(invoke).mockResolvedValue(result);
      render(<ResetSection pane={pane} />);

      await act(async () => {
        fireEvent.click(resetButton());
      });

      expect(invoke).toHaveBeenCalledWith("reset_app_data");
      expect(toast.success).toHaveBeenCalledWith("App data reset successfully. Restarting...");
      expect(toast.error).not.toHaveBeenCalled();
      expect(screen.getByRole("button", { name: pane ? "Resetting…" : "Resetting..." })).toBeDisabled();
      expect(relaunch).not.toHaveBeenCalled();
      await act(() => vi.advanceTimersByTime(999));
      expect(relaunch).not.toHaveBeenCalled();
      await act(() => vi.advanceTimersByTime(1));
      expect(relaunch).toHaveBeenCalledOnce();
    });
  });

  it("explains that the license and API keys survive the reset", async () => {
    render(<ResetSection />);

    expect(screen.getByText("Keep your license and API keys")).toBeInTheDocument();
    expect(screen.getByText("Delete all transcription history")).toBeInTheDocument();
    expect(screen.getByText("Remove all downloaded models")).toBeInTheDocument();

    await userEvent.setup().click(screen.getByRole("button", { name: "Reset App Data" }));

    expect(ask).toHaveBeenCalledWith(
      "This action cannot be undone. This will permanently delete all your Voicetypr data except your license and API keys.\n\nThe app will restart after reset.\n\nAre you absolutely sure?",
      expect.objectContaining({ title: "Reset App Data", kind: "warning" }),
    );
    expect(invoke).not.toHaveBeenCalled();
  });
});
