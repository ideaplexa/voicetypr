import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RecordingIndicatorCard } from "../recording/RecordingIndicatorCard";
import type { AppSettings } from "@/types";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn<(cmd: string) => Promise<unknown>>(),
  emit: vi.fn(() => Promise.resolve()),
  updateSettings: vi.fn(() => Promise.resolve()),
  refreshSettings: vi.fn(() => Promise.resolve()),
}));

let mockSettings: AppSettings;
let streams = false;

vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string) => mocks.invoke(cmd) }));
vi.mock("@tauri-apps/api/event", () => ({
  emit: mocks.emit,
  listen: vi.fn().mockResolvedValue(() => undefined),
}));
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: mockSettings,
    updateSettings: mocks.updateSettings,
    refreshSettings: mocks.refreshSettings,
  }),
}));
vi.mock("@/components/sections/recording/IslandPreview", () => ({ IslandPreview: () => null }));

describe("RecordingIndicatorCard live words", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSettings = {
      hotkey: "x",
      current_model: "parakeet-tdt-0.6b-v3",
      current_model_engine: "parakeet",
      speech_language: "en",
      theme: "system",
      pill_indicator_mode: "always",
      transcription_mode: "live_preview",
    };
    mocks.invoke.mockImplementation(async (cmd) =>
      cmd === "get_active_stream_capabilities"
        ? { capabilities: { supports_streaming: streams } }
        : null,
    );
  });

  it("is active and toggles when the model streams and Real-time words is on", async () => {
    streams = true;
    render(<RecordingIndicatorCard />);
    const toggle = screen.getByRole("switch", { name: "Show live words while you talk" });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).toBeChecked();
    await userEvent.click(toggle);
    expect(mocks.updateSettings).toHaveBeenCalledWith({ island_live_words: false });
  });

  it("is disabled with a link to Transcription when the model does not stream", async () => {
    streams = false;
    render(<RecordingIndicatorCard />);
    const toggle = screen.getByRole("switch", { name: "Show live words while you talk" });
    expect(toggle).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(screen.getByRole("button", { name: "Turn on Real-time words in Transcription" }));
    expect(mocks.emit).toHaveBeenCalledWith("navigate-to-section", "transcription");
  });

  it("is disabled when Real-time words is off", async () => {
    streams = true;
    mockSettings = { ...mockSettings, transcription_mode: "regular" };
    render(<RecordingIndicatorCard />);
    const toggle = screen.getByRole("switch", { name: "Show live words while you talk" });
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalled());
    expect(toggle).toHaveAttribute("aria-disabled", "true");
  });
});
