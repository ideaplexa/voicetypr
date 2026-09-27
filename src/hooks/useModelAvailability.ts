import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSettings } from "@/contexts/SettingsContext";
import type { ModelInfo } from "@/types";
import { createLogger } from "@/lib/logger";

const log = createLogger("model-availability");
export type RemoteAvailabilityStatus =
  | "unknown"
  | "online"
  | "offline"
  | "auth_failed"
  | "self_connection";

type RemoteAvailabilityStatusPayload =
  | RemoteAvailabilityStatus
  | "Unknown"
  | "Online"
  | "Offline"
  | "AuthFailed"
  | "SelfConnection";

interface ModelStatusResponse {
  models: ModelInfo[];
}

interface RecognitionAvailabilitySnapshot {
  whisper_available: boolean;
  parakeet_available: boolean;
  cloud_selected: boolean;
  cloud_ready: boolean;
  remote_selected: boolean;
  remote_status?: RemoteAvailabilityStatusPayload;
  remote_available: boolean;
  remote_last_checked?: number | string | null;
}

interface ModelAvailabilityState {
  hasModels: boolean | null;
  selectedModelAvailable: boolean | null;
  remoteSelected: boolean;
  remoteAvailable: boolean;
  remoteStatus: RemoteAvailabilityStatus;
  remoteLastChecked: number | string | null;
  isChecking: boolean;
}

/** Snapshot of model availability — the subset without isChecking/checkModels. */
export interface ModelAvailabilitySnapshot {
  hasModels: boolean | null;
  selectedModelAvailable: boolean | null;
  remoteSelected: boolean;
  remoteAvailable: boolean;
  remoteStatus: RemoteAvailabilityStatus;
  remoteLastChecked: number | string | null;
}

/** Full return type of useModelAvailability. Import this type at consumers. */
export interface ModelAvailability extends ModelAvailabilitySnapshot {
  isChecking: boolean;
  checkModels: () => Promise<ModelAvailabilitySnapshot>;
}

type DerivedAvailabilityState = ModelAvailabilitySnapshot;

const FALLBACK_AVAILABILITY: RecognitionAvailabilitySnapshot = {
  whisper_available: false,
  parakeet_available: false,
  cloud_selected: false,
  cloud_ready: false,
  remote_selected: false,
  remote_status: "unknown",
  remote_available: false,
  remote_last_checked: null,
};

const DEFAULT_STATE: ModelAvailabilityState = {
  hasModels: null,
  selectedModelAvailable: null,
  remoteSelected: false,
  remoteAvailable: false,
  remoteStatus: "unknown",
  remoteLastChecked: null,
  isChecking: false,
};

function getLocalSelectedModelAvailability(
  status: ModelStatusResponse,
  selectedModel?: string | null,
) {
  if (!selectedModel) {
    return false;
  }

  const match = status.models.find((model) => model.name === selectedModel);
  return !!match && match.downloaded && !match.requires_setup;
}

function normalizeRemoteStatus(
  snapshot: RecognitionAvailabilitySnapshot,
): RemoteAvailabilityStatus {
  switch (snapshot.remote_status) {
    case "online":
    case "Online":
      return "online";
    case "offline":
    case "Offline":
      return "offline";
    case "auth_failed":
    case "AuthFailed":
      return "auth_failed";
    case "self_connection":
    case "SelfConnection":
      return "self_connection";
    case "unknown":
    case "Unknown":
      return "unknown";
    default:
      if (!snapshot.remote_selected) {
        return "unknown";
      }

      return snapshot.remote_available ? "online" : "unknown";
  }
}

function deriveAvailabilityState(
  snapshot: RecognitionAvailabilitySnapshot,
  fallbackSelectedModelAvailable: boolean | null,
): DerivedAvailabilityState {
  const remoteSelected = snapshot.remote_selected;
  const remoteStatus = normalizeRemoteStatus(snapshot);
  const remoteAvailable = remoteStatus === "online";
  const hasLocalReadySource =
    snapshot.whisper_available ||
    snapshot.parakeet_available ||
    (snapshot.cloud_selected && snapshot.cloud_ready);

  const hasModels =
    remoteSelected && remoteStatus === "unknown"
      ? hasLocalReadySource
        ? true
        : null
      : hasLocalReadySource || remoteAvailable;

  const selectedModelAvailable = remoteSelected
    ? remoteStatus === "unknown"
      ? null
      : remoteAvailable
    : fallbackSelectedModelAvailable;

  return {
    hasModels,
    selectedModelAvailable,
    remoteSelected,
    remoteAvailable,
    remoteStatus,
    remoteLastChecked: snapshot.remote_last_checked ?? null,
  };
}

export function useModelAvailability() {
  const { settings } = useSettings();
  const selectedModel = settings?.current_model;
  const [state, setState] = useState<ModelAvailabilityState>(DEFAULT_STATE);
  const refreshGenerationRef = useRef(0);
  const latestAvailabilityRef = useRef<DerivedAvailabilityState>({
    hasModels: DEFAULT_STATE.hasModels,
    selectedModelAvailable: DEFAULT_STATE.selectedModelAvailable,
    remoteSelected: DEFAULT_STATE.remoteSelected,
    remoteAvailable: DEFAULT_STATE.remoteAvailable,
    remoteStatus: DEFAULT_STATE.remoteStatus,
    remoteLastChecked: DEFAULT_STATE.remoteLastChecked,
  });
  const remoteStateRef = useRef({
    remoteSelected: DEFAULT_STATE.remoteSelected,
    remoteStatus: DEFAULT_STATE.remoteStatus,
    remoteLastChecked: DEFAULT_STATE.remoteLastChecked,
  });

  useEffect(() => {
    remoteStateRef.current = {
      remoteSelected: state.remoteSelected,
      remoteStatus: state.remoteStatus,
      remoteLastChecked: state.remoteLastChecked,
    };
  }, [state.remoteSelected, state.remoteStatus, state.remoteLastChecked]);

  const applyCanonicalAvailability = useCallback(
    (snapshot: RecognitionAvailabilitySnapshot, fallbackSelectedModelAvailable: boolean | null) => {
      const derived = deriveAvailabilityState(snapshot, fallbackSelectedModelAvailable);
      latestAvailabilityRef.current = derived;
      remoteStateRef.current = {
        remoteSelected: derived.remoteSelected,
        remoteStatus: derived.remoteStatus,
        remoteLastChecked: derived.remoteLastChecked,
      };
      setState((current) => ({
        ...current,
        ...derived,
        isChecking: false,
      }));
      return derived;
    },
    [],
  );

  const checkModels = useCallback(async () => {
    const refreshGeneration = ++refreshGenerationRef.current;
    setState((current) => ({ ...current, isChecking: true }));

    try {
      const [status, availabilityResult] = await Promise.all([
        invoke<ModelStatusResponse>("get_model_status"),
        invoke<RecognitionAvailabilitySnapshot | null>("get_recognition_availability_snapshot"),
      ]);

      if (refreshGeneration !== refreshGenerationRef.current) {
        return latestAvailabilityRef.current;
      }

      const availability = availabilityResult ?? FALLBACK_AVAILABILITY;
      const localSelectedModelAvailable = getLocalSelectedModelAvailability(status, selectedModel);
      return applyCanonicalAvailability(availability, localSelectedModelAvailable);
    } catch (error) {
      log.error("Failed to check model availability:", error);
      if (refreshGeneration !== refreshGenerationRef.current) {
        return latestAvailabilityRef.current;
      }

      const unresolved: DerivedAvailabilityState = {
        hasModels: null,
        selectedModelAvailable: null,
        remoteSelected: remoteStateRef.current.remoteSelected,
        remoteAvailable: false,
        remoteStatus: "unknown",
        remoteLastChecked: remoteStateRef.current.remoteLastChecked,
      };
      latestAvailabilityRef.current = unresolved;
      remoteStateRef.current = {
        remoteSelected: unresolved.remoteSelected,
        remoteStatus: unresolved.remoteStatus,
        remoteLastChecked: unresolved.remoteLastChecked,
      };
      setState((current) => ({
        ...current,
        ...unresolved,
        isChecking: false,
      }));
      return unresolved;
    }
  }, [applyCanonicalAvailability, selectedModel]);

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      void checkModels();
    }, 0);

    return () => window.clearTimeout(timeoutId);
  }, [checkModels]);

  useEffect(() => {
    const handleFocus = () => {
      const { remoteSelected, remoteStatus } = remoteStateRef.current;
      if (remoteSelected && remoteStatus !== "online") {
        void invoke("refresh_active_remote_server_status");
      }
    };

    window.addEventListener("focus", handleFocus);
    const unlistenTauriFocus = listen("tauri://focus", handleFocus);

    return () => {
      window.removeEventListener("focus", handleFocus);
      unlistenTauriFocus.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    const unlistenDownloaded = listen("model-downloaded", () => {
      checkModels();
    });

    const unlistenDeleted = listen("model-deleted", () => {
      checkModels();
    });

    const unlistenModelChanged = listen("model-changed", () => {
      checkModels();
    });

    const unlistenCloudSaved = listen("stt-key-saved", () => {
      checkModels();
    });

    const unlistenCloudRemoved = listen("stt-key-removed", () => {
      checkModels();
    });

    const unlistenSharingChanged = listen("sharing-status-changed", () => {
      checkModels();
    });

    const unlistenAvailability = listen("recognition-availability", (event) => {
      refreshGenerationRef.current += 1;
      applyCanonicalAvailability(
        event.payload as RecognitionAvailabilitySnapshot,
        state.selectedModelAvailable,
      );
    });

    return () => {
      Promise.all([
        unlistenDownloaded,
        unlistenDeleted,
        unlistenModelChanged,
        unlistenCloudSaved,
        unlistenCloudRemoved,
        unlistenSharingChanged,
        unlistenAvailability,
      ]).then((unsubs) => {
        unsubs.forEach((unsub) => unsub());
      });
    };
  }, [applyCanonicalAvailability, checkModels, state.selectedModelAvailable]);

  return {
    hasModels: state.hasModels,
    selectedModelAvailable: state.selectedModelAvailable,
    remoteSelected: state.remoteSelected,
    remoteAvailable: state.remoteAvailable,
    remoteStatus: state.remoteStatus,
    remoteLastChecked: state.remoteLastChecked,
    isChecking: state.isChecking,
    checkModels,
  };
}
