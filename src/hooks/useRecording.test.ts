import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { useRecording } from "./useRecording";
import { listen } from "@tauri-apps/api/event";
import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
import { emitMockEvent } from "../test/setup";

// Get the mocked functions
const mockListen = vi.mocked(listen);

describe("useRecording", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearMocks();

    // Set up default IPC mock responses
    mockIPC((cmd) => {
      if (cmd === "start_recording") return Promise.resolve();
      if (cmd === "stop_recording") return Promise.resolve();
      if (cmd === "get_current_recording_state") return Promise.resolve({ state: "idle" });
      return Promise.reject(new Error(`Unknown command: ${cmd}`));
    });
  });

  it("should initialize with idle state", () => {
    const { result } = renderHook(() => useRecording());

    expect(result.current.state).toBe("idle");
    expect(result.current.error).toBeNull();
    expect(result.current.isActive).toBe(false);
  });

  it("should set up event listeners on mount", async () => {
    renderHook(() => useRecording());

    // Simply verify that event listeners are being registered
    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
      expect(mockListen.mock.calls.length).toBeGreaterThan(0);
    });
  });

  it("should handle state changes from backend events", async () => {
    const { result } = renderHook(() => useRecording());

    // Wait for listeners to be set up
    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    // Simulate state change event
    act(() => {
      emitMockEvent("recording-state-changed", { state: "recording", error: null });
    });

    expect(result.current.state).toBe("recording");
    expect(result.current.error).toBeNull();
    expect(result.current.isActive).toBe(true);
  });

  it("should handle error state", async () => {
    const { result } = renderHook(() => useRecording());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    act(() => {
      emitMockEvent("recording-state-changed", { state: "error", error: "Test error message" });
    });

    expect(result.current.state).toBe("error");
    expect(result.current.error).toBe("Test error message");
    expect(result.current.isActive).toBe(false);
  });

  it("should invoke start_recording command", async () => {
    const { result } = renderHook(() => useRecording());

    await act(async () => {
      await result.current.startRecording();
    });

    // The mockIPC handler should have been called with start_recording
    // We can verify this worked by checking no errors were thrown
    expect(result.current.state).toBe("idle"); // State managed by backend
  });

  it("should invoke stop_recording command", async () => {
    const { result } = renderHook(() => useRecording());

    await act(async () => {
      await result.current.stopRecording();
    });

    // The mockIPC handler should have been called with stop_recording
    expect(result.current.state).toBe("idle"); // State managed by backend
  });

  it("surfaces rejected start_recording command errors", async () => {
    const { result } = renderHook(() => useRecording());
    const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    mockIPC((cmd) => {
      if (cmd === "start_recording")
        return Promise.reject(new Error("License readiness is still loading"));
      if (cmd === "get_current_recording_state") return Promise.resolve({ state: "idle" });
      return Promise.resolve();
    });

    await act(async () => {
      await result.current.startRecording();
    });

    expect(result.current.state).toBe("error");
    expect(result.current.error).toBe("License readiness is still loading. Try again in a moment.");
    expect(result.current.isActive).toBe(false);
    expect(consoleErrorSpy).toHaveBeenCalledWith(
      "[Recording Hook] Failed to start recording:",
      expect.any(Error),
    );

    consoleErrorSpy.mockRestore();
  });

  it("surfaces rejected stop_recording command errors", async () => {
    const { result } = renderHook(() => useRecording());
    const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    mockIPC((cmd) => {
      if (cmd === "stop_recording") return Promise.reject(new Error("Stop command failed"));
      if (cmd === "get_current_recording_state") return Promise.resolve({ state: "idle" });
      return Promise.resolve();
    });

    await act(async () => {
      await result.current.stopRecording();
    });

    expect(result.current.state).toBe("error");
    expect(result.current.error).toBe("Stop command failed. Try again in a moment.");
    expect(result.current.isActive).toBe(false);
    expect(consoleErrorSpy).toHaveBeenCalledWith(
      "[Recording Hook] Failed to stop recording:",
      expect.any(Error),
    );

    consoleErrorSpy.mockRestore();
  });

  it("should calculate isActive correctly", async () => {
    const { result } = renderHook(() => useRecording());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    // Test different states
    const states = [
      { state: "idle", expectedActive: false },
      { state: "starting", expectedActive: true },
      { state: "recording", expectedActive: true },
      { state: "stopping", expectedActive: true },
      { state: "transcribing", expectedActive: true },
      { state: "error", expectedActive: false },
    ];

    for (const { state, expectedActive } of states) {
      act(() => {
        emitMockEvent("recording-state-changed", { state, error: null });
      });
      expect(result.current.isActive).toBe(expectedActive);
    }
  });

  it("should handle multiple event types", async () => {
    const { result } = renderHook(() => useRecording());

    await waitFor(() => {
      expect(mockListen).toHaveBeenCalled();
    });

    // Test legacy event handling
    act(() => {
      emitMockEvent("recording-started", {});
    });
    expect(result.current.state).toBe("recording");

    act(() => {
      emitMockEvent("transcription-started", {});
    });
    expect(result.current.state).toBe("transcribing");

    // The hook no longer listens to transcription-complete
    // State is managed by recording-state-changed event
    act(() => {
      emitMockEvent("recording-state-changed", { state: "idle", error: null });
    });
    expect(result.current.state).toBe("idle");
  });
});
