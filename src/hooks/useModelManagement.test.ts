import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { useModelManagement } from "./useModelManagement";

const mockInvoke = vi.fn();
const eventHandlers = new Map<string, (payload: unknown) => void | Promise<void>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => vi.fn()),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    info: vi.fn(),
    success: vi.fn(),
  },
}));

vi.mock("./useEventCoordinator", () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn(
      async (eventName: string, handler: (payload: unknown) => void | Promise<void>) => {
        eventHandlers.set(eventName, handler);
        return () => {
          eventHandlers.delete(eventName);
        };
      },
    ),
  }),
}));

const parakeetModel = {
  name: "parakeet-tdt-0.6b-v3",
  display_name: "Parakeet V3",
  size: 1,
  url: "",
  sha256: "",
  downloaded: false,
  speed_score: 10,
  accuracy_score: 10,
  recommended: true,
  engine: "parakeet",
};

const emitModelEvent = async (eventName: string, payload: unknown) => {
  const handler = eventHandlers.get(eventName);
  expect(handler).toBeDefined();
  await handler?.(payload);
};

const getDownloadRequestId = (index: number) => {
  const downloadCalls = mockInvoke.mock.calls.filter(([command]) => command === "download_model");
  const payload = downloadCalls[index]?.[1] as { requestId?: string } | undefined;
  const requestId = payload?.requestId;
  expect(requestId).toBeTruthy();
  return requestId as string;
};

describe("useModelManagement", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventHandlers.clear();
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_model_status") {
        return Promise.resolve({ models: [parakeetModel] });
      }

      if (command === "download_model") {
        return new Promise(() => undefined);
      }

      if (command === "cancel_download") {
        return Promise.resolve(null);
      }

      return Promise.resolve(null);
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("does not show a downloaded toast when completion arrives after cancellation", async () => {
    const { result } = renderHook(() => useModelManagement());

    await waitFor(() => {
      expect(result.current.models["parakeet-tdt-0.6b-v3"]).toBeDefined();
      expect(eventHandlers.has("model-downloaded")).toBe(true);
      expect(eventHandlers.has("download-cancelled")).toBe(true);
    });

    await act(async () => {
      await result.current.downloadModel("parakeet-tdt-0.6b-v3");
    });

    await act(async () => {
      await result.current.cancelDownload("parakeet-tdt-0.6b-v3");
    });

    await act(async () => {
      await emitModelEvent("download-cancelled", { model: "parakeet-tdt-0.6b-v3" });
      await emitModelEvent("model-downloaded", { model: "parakeet-tdt-0.6b-v3" });
    });

    expect(toast.info).toHaveBeenCalledWith("Download cancelled for Parakeet V3");
    expect(toast.success).not.toHaveBeenCalledWith("Parakeet V3 downloaded successfully");
    expect(result.current.downloadProgress).not.toHaveProperty("parakeet-tdt-0.6b-v3");
  });

  it("keeps cancellation active until acknowledgement and ignores stale success", async () => {
    const { result } = renderHook(() => useModelManagement());

    await waitFor(() => {
      expect(result.current.models["parakeet-tdt-0.6b-v3"]).toBeDefined();
      expect(eventHandlers.has("model-downloaded")).toBe(true);
      expect(eventHandlers.has("download-cancelled")).toBe(true);
    });

    await act(async () => {
      await result.current.downloadModel("parakeet-tdt-0.6b-v3");
    });
    const cancelledRequestId = getDownloadRequestId(0);

    await act(async () => {
      await result.current.cancelDownload("parakeet-tdt-0.6b-v3");
      await result.current.downloadModel("parakeet-tdt-0.6b-v3");
    });

    expect(mockInvoke.mock.calls.filter(([command]) => command === "download_model")).toHaveLength(
      1,
    );
    expect(toast.info).toHaveBeenCalledWith("Parakeet V3 is already downloading");

    await act(async () => {
      await emitModelEvent("model-downloaded", {
        model: "parakeet-tdt-0.6b-v3",
        requestId: cancelledRequestId,
      });
      await emitModelEvent("download-cancelled", {
        model: "parakeet-tdt-0.6b-v3",
        requestId: cancelledRequestId,
      });
    });

    expect(toast.success).not.toHaveBeenCalledWith("Parakeet V3 downloaded successfully");

    await act(async () => {
      await result.current.downloadModel("parakeet-tdt-0.6b-v3");
    });
    const retryRequestId = getDownloadRequestId(1);

    await act(async () => {
      await emitModelEvent("model-downloaded", {
        model: "parakeet-tdt-0.6b-v3",
        requestId: retryRequestId,
      });
    });

    expect(toast.success).toHaveBeenCalledWith("Parakeet V3 downloaded successfully");
  });

  it("records download errors per model and clears verifying state", async () => {
    const { result } = renderHook(() => useModelManagement());

    await waitFor(() => {
      expect(result.current.models["parakeet-tdt-0.6b-v3"]).toBeDefined();
      expect(eventHandlers.has("model-verifying")).toBe(true);
      expect(eventHandlers.has("download-error")).toBe(true);
    });

    await act(async () => {
      await result.current.downloadModel("parakeet-tdt-0.6b-v3");
    });
    const requestId = getDownloadRequestId(0);

    await act(async () => {
      await emitModelEvent("model-verifying", {
        model: "parakeet-tdt-0.6b-v3",
        requestId,
      });
      await emitModelEvent("download-error", {
        model: "parakeet-tdt-0.6b-v3",
        requestId,
        error: "verification_failed",
      });
    });

    expect(result.current.downloadErrors["parakeet-tdt-0.6b-v3"]).toBe("verification_failed");
    expect(result.current.downloadProgress).not.toHaveProperty("parakeet-tdt-0.6b-v3");
    expect(result.current.downloadPhases).not.toHaveProperty("parakeet-tdt-0.6b-v3");
    expect(result.current.verifyingModels.has("parakeet-tdt-0.6b-v3")).toBe(false);
  });

  it("does not let an old verification timer hide a retry progress update", async () => {
    const { result } = renderHook(() => useModelManagement());

    await waitFor(() => {
      expect(result.current.models["parakeet-tdt-0.6b-v3"]).toBeDefined();
      expect(eventHandlers.has("model-verifying")).toBe(true);
      expect(eventHandlers.has("download-progress")).toBe(true);
    });

    vi.useFakeTimers();

    await act(async () => {
      await result.current.downloadModel("parakeet-tdt-0.6b-v3");
    });
    const staleRequestId = getDownloadRequestId(0);

    await act(async () => {
      await emitModelEvent("model-verifying", {
        model: "parakeet-tdt-0.6b-v3",
        requestId: staleRequestId,
      });
      await result.current.cancelDownload("parakeet-tdt-0.6b-v3");
      await emitModelEvent("download-cancelled", {
        model: "parakeet-tdt-0.6b-v3",
        requestId: staleRequestId,
      });
      await result.current.downloadModel("parakeet-tdt-0.6b-v3");
    });
    const retryRequestId = getDownloadRequestId(1);

    await act(async () => {
      await emitModelEvent("download-progress", {
        model: "parakeet-tdt-0.6b-v3",
        downloaded: 25,
        total: 100,
        progress: 25,
        requestId: retryRequestId,
        phase: "downloading 1/4",
      });
    });

    expect(result.current.downloadProgress["parakeet-tdt-0.6b-v3"]).toBe(25);

    await act(async () => {
      vi.advanceTimersByTime(500);
    });

    expect(result.current.downloadProgress["parakeet-tdt-0.6b-v3"]).toBe(25);
    expect(result.current.downloadPhases["parakeet-tdt-0.6b-v3"]).toBe("downloading 1/4");
  });

  it("does not double-handle a failure delivered via both the command rejection and the download-error event", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_model_status") {
        return Promise.resolve({ models: [parakeetModel] });
      }
      if (command === "download_model") {
        return Promise.reject(new Error("verification_failed"));
      }
      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useModelManagement());

    await waitFor(() => {
      expect(result.current.models["parakeet-tdt-0.6b-v3"]).toBeDefined();
      expect(eventHandlers.has("download-error")).toBe(true);
    });

    await act(async () => {
      await result.current.downloadModel("parakeet-tdt-0.6b-v3");
    });

    const requestId = getDownloadRequestId(0);

    // The command rejection handles the failure first: exactly one error toast.
    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledTimes(1);
    });

    // The backend also emits download-error for the SAME request; it must NOT toast again.
    await act(async () => {
      await emitModelEvent("download-error", {
        model: "parakeet-tdt-0.6b-v3",
        requestId,
        error: "verification_failed",
      });
    });

    expect(toast.error).toHaveBeenCalledTimes(1);
    expect(result.current.downloadErrors["parakeet-tdt-0.6b-v3"]).toBeTruthy();
  });
});
