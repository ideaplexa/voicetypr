import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ask } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { ResetSection } from "../ResetSection";

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ updateSettings: vi.fn() }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("ResetSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(ask).mockResolvedValue(false);
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
