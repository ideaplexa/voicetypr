import { StrictMode } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { MicrophoneSelection } from "./MicrophoneSelection";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: { info: vi.fn(), error: vi.fn() } }));

describe("MicrophoneSelection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listen).mockResolvedValue(() => {});
  });

  it("announces one stored-device reset under StrictMode", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "validate_microphone_selection") return true;
      if (command === "get_audio_devices") return ["USB Microphone"];
      return null;
    });

    render(
      <StrictMode>
        <MicrophoneSelection value="Built-in Microphone" onValueChange={vi.fn()} />
      </StrictMode>,
    );

    await waitFor(() => expect(screen.getByRole("combobox")).not.toBeDisabled());
    expect(toast.info).toHaveBeenCalledTimes(1);
    expect(toast.info).toHaveBeenCalledWith(
      "Previously selected microphone is no longer available, using default",
      { id: "microphone-selection-unavailable" },
    );
  });

  it("resets a device missing from repeated lists once", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "validate_microphone_selection") return false;
      if (command === "get_audio_devices") return ["USB Microphone"];
      return null;
    });
    const onValueChange = vi.fn();
    render(<MicrophoneSelection value="Built-in Microphone" onValueChange={onValueChange} />);

    await waitFor(() => expect(onValueChange).toHaveBeenCalledTimes(1));
    expect(onValueChange).toHaveBeenCalledWith(undefined);
    expect(toast.info).toHaveBeenCalledWith(
      "Built-in Microphone is no longer available, switching to default microphone",
      { id: "microphone-selection-unavailable" },
    );
  });
});
