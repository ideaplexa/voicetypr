import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { emitMockEvent } from "./setup";

// Example component that uses Tauri commands
function RecordButton() {
  const [isRecording, setIsRecording] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    // Listen for recording state changes
    const unsubscribe = listen("recording-state-changed", (event: any) => {
      if (event.payload.state === "recording") {
        setIsRecording(true);
      } else if (event.payload.state === "idle") {
        setIsRecording(false);
      }

      if (event.payload.error) {
        setError(event.payload.error);
      }
    });

    return () => {
      unsubscribe.then((fn) => fn());
    };
  }, []);

  const handleClick = async () => {
    try {
      if (isRecording) {
        await invoke("stop_recording");
      } else {
        await invoke("start_recording");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    }
  };

  return (
    <div>
      <button onClick={handleClick}>{isRecording ? "Stop Recording" : "Start Recording"}</button>
      {error && <div role="alert">{error}</div>}
    </div>
  );
}

import React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

describe("Tauri Integration Example", () => {
  beforeEach(() => {
    clearMocks();
    vi.clearAllMocks();
  });

  it("should handle recording toggle with Tauri commands", async () => {
    const user = userEvent.setup();

    // Mock the IPC responses
    mockIPC((cmd) => {
      if (cmd === "start_recording") {
        // Simulate backend emitting state change after command
        setTimeout(() => {
          emitMockEvent("recording-state-changed", {
            state: "recording",
            error: null,
          });
        }, 10);
        return Promise.resolve();
      }

      if (cmd === "stop_recording") {
        setTimeout(() => {
          emitMockEvent("recording-state-changed", {
            state: "idle",
            error: null,
          });
        }, 10);
        return Promise.resolve();
      }

      return Promise.reject(new Error(`Unknown command: ${cmd}`));
    });

    render(<RecordButton />);

    const button = screen.getByRole("button");
    expect(button).toHaveTextContent("Start Recording");

    // Click to start recording
    await user.click(button);

    // Wait for state update from event
    await waitFor(() => {
      expect(button).toHaveTextContent("Stop Recording");
    });

    // Click to stop recording
    await user.click(button);

    // Wait for state update
    await waitFor(() => {
      expect(button).toHaveTextContent("Start Recording");
    });
  });

  it("should handle command errors", async () => {
    const user = userEvent.setup();

    // Mock IPC to return an error
    mockIPC((cmd) => {
      if (cmd === "start_recording") {
        return Promise.reject(new Error("Microphone not available"));
      }
      return Promise.resolve();
    });

    render(<RecordButton />);

    const button = screen.getByRole("button");
    await user.click(button);

    // Check error is displayed
    await waitFor(() => {
      expect(screen.getByRole("alert")).toHaveTextContent("Microphone not available");
    });
  });

  it("should track IPC calls", async () => {
    const user = userEvent.setup();
    const ipcSpy = vi.fn();

    // Mock IPC with spy
    mockIPC((cmd, args) => {
      ipcSpy(cmd, args);
      return Promise.resolve();
    });

    render(<RecordButton />);

    const button = screen.getByRole("button");
    await user.click(button);

    // Verify IPC was called (Tauri sends empty object when no args)
    expect(ipcSpy).toHaveBeenCalledWith("start_recording", {});
  });
});
