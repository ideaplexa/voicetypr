import { render, screen, waitFor, act } from "@testing-library/react";
import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";
import { RecordingsTab } from "./RecordingsTab";
import { emit } from "@tauri-apps/api/event";

// Mock sonner
vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

// Mock hooks
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: { hotkey: "Cmd+Shift+Space" },
  }),
}));

// Track registered events
const registeredEvents: Record<string, any> = {};

// Mock Tauri core invoke so we don't depend on window.__TAURI_INTERNALS__
let invokeMock = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<any>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
}));

// Use real useEventCoordinator; events are mocked globally in setup

// Mock RecentRecordings component
vi.mock("@/components/sections/RecentRecordings", () => ({
  RecentRecordings: ({ history, onHistoryUpdate, isLoading, loadError }: any) => (
    <div data-testid="recent-recordings">
      <div>History count: {history.length}</div>
      {isLoading ? <div>Loading history</div> : null}
      {loadError ? <div>{loadError}</div> : null}
      <button onClick={onHistoryUpdate}>Refresh</button>
      {history.map((item: any) => (
        <div key={item.id}>{item.text}</div>
      ))}
    </div>
  ),
}));

describe("RecordingsTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (window as any).__testEventCallbacks = {};
    Object.keys(registeredEvents).forEach((key) => delete registeredEvents[key]);
    invokeMock = vi.fn(async (cmd: string) => {
      if (cmd === "get_transcription_history") return [];
      return null;
    });
  });

  afterEach(() => {
    // no-op; mocks reset in beforeEach
  });

  it("loads history on mount", async () => {
    const mockHistory = [
      {
        timestamp: "2024-01-01T00:00:00Z",
        text: "Test transcription",
        model: "tiny",
      },
    ];

    // Setup mock for this test
    invokeMock = vi.fn(async (cmd: string) => {
      if (cmd === "get_transcription_history") return mockHistory;
      return null;
    });

    render(<RecordingsTab />);

    await waitFor(() => {
      expect(screen.getByText("History count: 1")).toBeInTheDocument();
    });
  });

  it("displays empty state when no history", async () => {
    render(<RecordingsTab />);

    await waitFor(() => {
      expect(screen.getByText("History count: 0")).toBeInTheDocument();
    });
  });

  it("surfaces errors when loading history", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    invokeMock = vi.fn(async (cmd: string) => {
      if (cmd === "get_transcription_history") throw new Error("Failed to load");
      return null;
    });

    render(<RecordingsTab />);

    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalledWith(
        "Failed to load transcription history:",
        expect.any(Error),
      );
      expect(screen.getByText("Couldn't load history")).toBeInTheDocument();
    });

    consoleSpy.mockRestore();
  });

  it("reloads history when history-updated event is fired", async () => {
    const mockHistory = [
      {
        timestamp: "2024-01-01T00:00:00Z",
        text: "Updated transcription",
        model: "base",
      },
    ];

    // Setup mock to return empty first, then updated history
    let callCount = 0;
    invokeMock = vi.fn(async (cmd: string) => {
      if (cmd === "get_transcription_history") {
        callCount++;
        return callCount === 1 ? [] : mockHistory;
      }
      return null;
    });

    render(<RecordingsTab />);

    // Initially empty
    await waitFor(() => {
      expect(screen.getByText("History count: 0")).toBeInTheDocument();
    });

    // Fire the event (behavioral path)
    await act(async () => {
      await emit("history-updated");
    });

    // Should reload and show updated history
    await waitFor(() => {
      expect(screen.getByText("History count: 1")).toBeInTheDocument();
    });
  });

  it("reloads history when transcription-updated event is fired", async () => {
    const mockHistory = [
      {
        timestamp: "2024-01-01T00:00:00Z",
        text: "Re-transcribed text",
        model: "Remote: Office PC",
      },
    ];

    let callCount = 0;
    invokeMock = vi.fn(async (cmd: string) => {
      if (cmd === "get_transcription_history") {
        callCount++;
        return callCount === 1 ? [] : mockHistory;
      }
      return null;
    });

    render(<RecordingsTab />);

    await waitFor(() => {
      expect(screen.getByText("History count: 0")).toBeInTheDocument();
    });

    await act(async () => {
      await emit("transcription-updated");
    });

    await waitFor(() => {
      expect(screen.getByText("History count: 1")).toBeInTheDocument();
    });
  });
});
