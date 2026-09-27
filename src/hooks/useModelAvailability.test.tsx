import { renderHook, act, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useModelAvailability } from "./useModelAvailability";

const mockInvoke = vi.fn();
const tauriEventListeners = new Map<string, Set<(event: { payload: unknown }) => void>>();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (eventName: string, callback: (event: { payload: unknown }) => void) => {
    const listeners =
      tauriEventListeners.get(eventName) ?? new Set<(event: { payload: unknown }) => void>();
    listeners.add(callback);
    tauriEventListeners.set(eventName, listeners);

    return () => {
      listeners.delete(callback);
    };
  }),
}));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: {
      current_model: "local-model",
    },
  }),
}));

const emitTauriEvent = (eventName: string, payload: unknown) => {
  for (const callback of tauriEventListeners.get(eventName) ?? []) {
    callback({ payload });
  }
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve;
    reject = promiseReject;
  });

  return { promise, resolve, reject };
}

const unknownRemoteSnapshot = {
  whisper_available: false,
  parakeet_available: false,
  cloud_selected: false,
  cloud_ready: false,
  remote_selected: true,
  remote_status: "unknown",
  remote_available: false,
  remote_last_checked: null,
};

const onlineRemoteSnapshot = {
  whisper_available: false,
  parakeet_available: false,
  cloud_selected: false,
  cloud_ready: false,
  remote_selected: true,
  remote_status: "online",
  remote_available: true,
  remote_last_checked: "2026-03-31T00:01:00Z",
};

beforeEach(() => {
  vi.clearAllMocks();
  tauriEventListeners.clear();
  mockInvoke.mockImplementation((command: string) => {
    if (command === "get_model_status") {
      return Promise.resolve({ models: [] });
    }

    if (command === "get_recognition_availability_snapshot") {
      return Promise.resolve(unknownRemoteSnapshot);
    }

    if (command === "refresh_active_remote_server_status") {
      return Promise.resolve(null);
    }

    return Promise.resolve(null);
  });
});

describe("useModelAvailability", () => {
  it("treats a selected remote with unknown status as unresolved rather than unavailable", async () => {
    const { result } = renderHook(() => useModelAvailability());

    await waitFor(() => {
      expect(result.current.remoteSelected).toBe(true);
      expect(result.current.remoteStatus).toBe("unknown");
    });

    expect(result.current.remoteAvailable).toBe(false);
    expect(result.current.selectedModelAvailable).toBeNull();
    expect(result.current.hasModels).toBeNull();
  });

  it("treats auth-failed remote status as resolved and unavailable", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_model_status") {
        return Promise.resolve({ models: [] });
      }

      if (command === "get_recognition_availability_snapshot") {
        return Promise.resolve({
          ...unknownRemoteSnapshot,
          remote_status: "AuthFailed",
          remote_available: false,
        });
      }

      if (command === "refresh_active_remote_server_status") {
        return Promise.resolve(null);
      }

      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useModelAvailability());

    await waitFor(() => {
      expect(result.current.remoteSelected).toBe(true);
      expect(result.current.remoteStatus).toBe("auth_failed");
    });

    expect(result.current.remoteAvailable).toBe(false);
    expect(result.current.selectedModelAvailable).toBe(false);
    expect(result.current.hasModels).toBe(false);
  });

  it("does not collapse availability failures into a confirmed no-model state", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_model_status") {
        return Promise.reject(new Error("temporary failure"));
      }

      if (command === "get_recognition_availability_snapshot") {
        return Promise.resolve(unknownRemoteSnapshot);
      }

      if (command === "refresh_active_remote_server_status") {
        return Promise.resolve(null);
      }

      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useModelAvailability());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    expect(result.current.hasModels).toBeNull();
    expect(result.current.selectedModelAvailable).toBeNull();
    expect(result.current.remoteStatus).toBe("unknown");
  });

  it("demotes a previously confirmed state back to unresolved when refresh fails", async () => {
    const { result } = renderHook(() => useModelAvailability());

    await waitFor(() => {
      expect(result.current.remoteStatus).toBe("unknown");
    });

    act(() => {
      emitTauriEvent("recognition-availability", onlineRemoteSnapshot);
    });

    await waitFor(() => {
      expect(result.current.remoteStatus).toBe("online");
      expect(result.current.hasModels).toBe(true);
    });

    mockInvoke.mockImplementation(() => Promise.reject(new Error("refresh failed")));

    await act(async () => {
      await result.current.checkModels();
    });

    expect(result.current.hasModels).toBeNull();
    expect(result.current.selectedModelAvailable).toBeNull();
    expect(result.current.remoteStatus).toBe("unknown");
    expect(result.current.remoteAvailable).toBe(false);
  });

  it("applies canonical availability events directly to hook state", async () => {
    const { result } = renderHook(() => useModelAvailability());

    await waitFor(() => {
      expect(result.current.isChecking).toBe(false);
    });

    act(() => {
      emitTauriEvent("recognition-availability", onlineRemoteSnapshot);
    });

    await waitFor(() => {
      expect(result.current.remoteStatus).toBe("online");
      expect(result.current.remoteAvailable).toBe(true);
      expect(result.current.selectedModelAvailable).toBe(true);
      expect(result.current.hasModels).toBe(true);
    });
  });

  it("does not let a stale refresh overwrite a newer recognition availability event", async () => {
    const statusDeferred = deferred<{ models: [] }>();
    const availabilityDeferred = deferred<typeof unknownRemoteSnapshot>();

    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_model_status") {
        return statusDeferred.promise;
      }

      if (command === "get_recognition_availability_snapshot") {
        return availabilityDeferred.promise;
      }

      if (command === "refresh_active_remote_server_status") {
        return Promise.resolve(null);
      }

      return Promise.resolve(null);
    });

    const { result } = renderHook(() => useModelAvailability());

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_model_status");
    });

    act(() => {
      emitTauriEvent("recognition-availability", onlineRemoteSnapshot);
    });

    await waitFor(() => {
      expect(result.current.remoteStatus).toBe("online");
    });

    await act(async () => {
      statusDeferred.resolve({ models: [] });
      availabilityDeferred.resolve(unknownRemoteSnapshot);
      await Promise.all([statusDeferred.promise, availabilityDeferred.promise]);
    });

    expect(result.current.remoteStatus).toBe("online");
    expect(result.current.remoteAvailable).toBe(true);
    expect(result.current.selectedModelAvailable).toBe(true);
    expect(result.current.hasModels).toBe(true);
  });

  it("refreshes the active remote only when it is selected and not already online", async () => {
    const { result } = renderHook(() => useModelAvailability());

    await waitFor(() => {
      expect(result.current.remoteSelected).toBe(true);
      expect(result.current.remoteStatus).toBe("unknown");
    });

    mockInvoke.mockClear();

    act(() => {
      window.dispatchEvent(new Event("focus"));
    });

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("refresh_active_remote_server_status");
    });

    mockInvoke.mockClear();

    act(() => {
      emitTauriEvent("recognition-availability", onlineRemoteSnapshot);
    });

    await waitFor(() => {
      expect(result.current.remoteStatus).toBe("online");
    });

    act(() => {
      window.dispatchEvent(new Event("focus"));
    });

    expect(mockInvoke).not.toHaveBeenCalledWith("refresh_active_remote_server_status");
  });
});
