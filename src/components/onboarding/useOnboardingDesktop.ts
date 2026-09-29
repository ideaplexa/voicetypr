import type { BareModifierSpec } from "@/components/HotkeyInput";
import {
  isRemoteServerOnline,
  type DiscoveredRemoteServer,
  type OnboardingDesktopProps,
  type PermissionState,
  type SourceType,
  type Step,
} from "@/components/onboarding/onboardingTypes";
import type { SavedConnection } from "@/components/RemoteServerCard";
import { useSettings } from "@/contexts/SettingsContext";
import { useAccessibilityPermission } from "@/hooks/useAccessibilityPermission";
import { useMicrophonePermission } from "@/hooks/useMicrophonePermission";
import { getCloudProviderByModel, isCloudEngine } from "@/lib/cloudProviders";
import { createLogger } from "@/lib/logger";
import { isMacOS } from "@/lib/platform";
import { findActivePrimaryBinding } from "@/lib/shortcut-display";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import type {
  ModifierKind,
  ModifierSide,
  ShortcutBinding,
  ShortcutSettings,
} from "@/types/shortcuts";
import { isCloudModel, isLocalModel, type ModelInfo } from "@/types";

const log = createLogger("onboarding");

export function useOnboardingDesktop({
  onCompletionStart,
  onCompletionError,
  onComplete,
  modelManagement,
  previewPhase,
}: OnboardingDesktopProps) {
  const { settings, updateSettings } = useSettings();
  const {
    hasPermission: hasMicPermission,
    checkPermission: checkMicPermission,
    requestPermission: requestMicPermission,
  } = useMicrophonePermission({ checkOnMount: false });
  const {
    hasPermission: hasAccessPermission,
    checkPermission: checkAccessPermission,
    requestPermission: requestAccessPermission,
  } = useAccessibilityPermission({ checkOnMount: false });

  const checkPermissions = useCallback(async () => {
    await Promise.all([checkMicPermission(), checkAccessPermission()]);
  }, [checkMicPermission, checkAccessPermission]);
  const checkPermissionsRef = useRef(checkPermissions);
  useEffect(() => {
    checkPermissionsRef.current = checkPermissions;
  });
  const {
    models,
    loadModels,
    modelOrder,
    downloadProgress,
    verifyingModels,
    downloadErrors = {},
    downloadModel,
    cancelDownload,
    deleteModel,
    repairModel,
    isLoading,
  } = modelManagement;

  const [currentStep, setCurrentStep] = useState<Step>(
    previewPhase === 2
      ? isMacOS
        ? "permissions"
        : "hotkey"
      : previewPhase === 3
        ? "success"
        : "welcome",
  );
  // Both independent privacy choices are opt-out and default to checked.
  const [telemetryOptIn, setTelemetryOptIn] = useState(true);
  const [analyticsOptIn, setAnalyticsOptIn] = useState(true);
  const [sourceType, setSourceType] = useState<SourceType>(() =>
    isCloudEngine(settings?.current_model_engine ?? "") ? "cloud" : "local",
  );
  const [hotkey, setHotkey] = useState(() => settings?.hotkey || "");
  const [isEditingHotkey, setIsEditingHotkey] = useState(false);
  const [capturedBareModifier, setCapturedBareModifier] = useState<BareModifierSpec | null>(null);
  const [isRequestingPermission, setIsRequestingPermission] = useState<string | null>(null);
  const [checkingPermissions, setCheckingPermissions] = useState<Set<string>>(new Set());
  const [remoteServers, setRemoteServers] = useState<SavedConnection[]>([]);
  const [activeRemoteServerId, setActiveRemoteServerId] = useState<string | null>(null);
  const [isLoadingRemoteServers, setIsLoadingRemoteServers] = useState(false);
  const [showAddRemoteModal, setShowAddRemoteModal] = useState(false);
  const [discoveredRemoteServers, setDiscoveredRemoteServers] = useState<DiscoveredRemoteServer[]>(
    [],
  );
  const [selectedDiscoveredServer, setSelectedDiscoveredServer] =
    useState<DiscoveredRemoteServer | null>(null);
  const [isSavingCompletion, setIsSavingCompletion] = useState(false);
  const [holdToTalk, setHoldToTalk] = useState(settings?.recording_mode === "push_to_talk");
  const [cloudModelSetup, setCloudModelSetup] = useState<string | null>(null);
  const [isSavingCloudKey, setIsSavingCloudKey] = useState(false);
  const [hotkeyHydrated, setHotkeyHydrated] = useState(() => Boolean(settings?.hotkey));
  const [previousSettings, setPreviousSettings] = useState(settings);
  const sourceChosenByUser = useRef(false);

  // Hydrate persisted consent so a relaunched onboarding cannot silently
  // overwrite a previously stored opt-out with the default-on state.
  useEffect(() => {
    let cancelled = false;
    Promise.all([
      invoke<{ enabled: boolean }>("get_telemetry_status"),
      invoke<{ enabled: boolean }>("get_product_analytics_status"),
    ])
      .then(([telemetry, analytics]) => {
        if (cancelled) return;
        setTelemetryOptIn(telemetry.enabled);
        setAnalyticsOptIn(analytics.enabled);
      })
      .catch((error) => {
        log.error("Failed to read stored privacy choices:", error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Mirror a hotkey that arrives with settings during render rather than
  // synchronously updating state from the settings effect below.
  if (settings !== previousSettings) {
    setPreviousSettings(settings);
    if (settings?.hotkey && !hotkeyHydrated) {
      setHotkey(settings.hotkey);
      setCapturedBareModifier(null);
      setHotkeyHydrated(true);
    }
  }

  const permissions = {
    microphone: {
      status: hasMicPermission === null ? "checking" : hasMicPermission ? "granted" : "denied",
    } as PermissionState,
    accessibility: {
      status:
        hasAccessPermission === null ? "checking" : hasAccessPermission ? "granted" : "denied",
    } as PermissionState,
  };

  const steps = useMemo(
    () =>
      isMacOS
        ? (["welcome", "source", "readiness", "permissions", "hotkey", "success"] satisfies Step[])
        : (["welcome", "source", "readiness", "hotkey", "success"] satisfies Step[]),
    [],
  );

  const currentIndex = steps.indexOf(currentStep);
  const selectedModelName = settings?.current_model || null;
  const selectedModel = selectedModelName ? models[selectedModelName] : null;
  const localModelNames = useMemo(
    () => modelOrder.filter((name) => models[name] && isLocalModel(models[name])),
    [modelOrder, models],
  );
  const cloudModelNames = useMemo(
    () => modelOrder.filter((name) => models[name] && isCloudModel(models[name])),
    [modelOrder, models],
  );
  const activeCloudProvider = cloudModelSetup
    ? getCloudProviderByModel(cloudModelSetup)
    : undefined;
  const activeRemoteServer = useMemo(
    () => remoteServers.find((server) => server.id === activeRemoteServerId) ?? null,
    [activeRemoteServerId, remoteServers],
  );
  const isModelReady = useCallback(
    (name: string) => models[name]?.downloaded === true && !models[name]?.requires_setup,
    [models],
  );
  const handleDeleteModel = useCallback(
    async (modelName: string) => {
      const deleted = await deleteModel(modelName);
      if (deleted && settings?.current_model === modelName) {
        await updateSettings({ current_model: "", current_model_engine: "whisper" });
      }
    },
    [deleteModel, settings, updateSettings],
  );
  const localReady = Boolean(
    sourceType === "local" &&
    selectedModelName &&
    selectedModel &&
    isLocalModel(selectedModel) &&
    isModelReady(selectedModelName),
  );
  const cloudReady = Boolean(
    sourceType === "cloud" &&
    selectedModelName &&
    selectedModel &&
    isCloudModel(selectedModel) &&
    isModelReady(selectedModelName),
  );
  const hasDownloadedLocalModel = localModelNames.some((name) => isModelReady(name));
  const remoteReady = sourceType === "remote" && isRemoteServerOnline(activeRemoteServer);
  const sourceReady =
    sourceType === "local" ? localReady : sourceType === "cloud" ? cloudReady : remoteReady;

  const loadRemoteServers = useCallback(async () => {
    setIsLoadingRemoteServers(true);
    try {
      const [savedServers, activeServer, discoveredServers] = await Promise.all([
        invoke<SavedConnection[]>("list_remote_servers"),
        invoke<string | null>("get_active_remote_server"),
        invoke<DiscoveredRemoteServer[]>("discover_remote_servers", { timeoutMs: 1200 }).catch(
          (error) => {
            log.error("[OnboardingDesktop] Failed to discover remote servers:", error);
            return [] as DiscoveredRemoteServer[];
          },
        ),
      ]);
      const discoveredCandidates = discoveredServers.filter(
        (server) =>
          !savedServers.some((saved) => saved.host === server.host && saved.port === server.port),
      );

      const allServers = savedServers;

      setActiveRemoteServerId(activeServer);
      setRemoteServers(allServers);
      setDiscoveredRemoteServers(discoveredCandidates);

      const refreshedServers = await Promise.all(
        allServers.map(async (server) => {
          try {
            return await invoke<SavedConnection>("check_remote_server_status", {
              serverId: server.id,
            });
          } catch (error) {
            log.error(`[OnboardingDesktop] Failed to refresh remote server ${server.id}:`, error);
            return server;
          }
        }),
      );

      setRemoteServers(refreshedServers);
    } catch (error) {
      log.error("[OnboardingDesktop] Failed to load remote servers:", error);
    } finally {
      setIsLoadingRemoteServers(false);
    }
  }, []);

  useEffect(() => {
    if (currentStep !== "permissions") return;
    void checkPermissions();
  }, [currentStep, checkPermissions]);

  useEffect(() => {
    if (currentStep !== "permissions") return;
    const handleFocus = () => {
      void checkPermissionsRef.current();
    };

    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, [currentStep]);

  useEffect(() => {
    if (currentStep !== "readiness" || sourceType !== "remote") return;
    void (async () => {
      await Promise.resolve();
      await loadRemoteServers();
    })();
  }, [currentStep, sourceType, loadRemoteServers]);

  useEffect(() => {
    let cancelled = false;
    void invoke<string | null>("get_active_remote_server")
      .then((serverId) => {
        if (!cancelled && serverId) {
          setActiveRemoteServerId(serverId);
          if (!sourceChosenByUser.current) {
            setSourceType("remote");
          }
        }
      })
      .catch((error) => {
        log.error("[OnboardingDesktop] Failed to restore active remote server:", error);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!settings || hotkeyHydrated) return;
    let cancelled = false;
    void invoke<ShortcutSettings>("get_shortcut_settings")
      .then((shortcutSettings) => {
        if (cancelled) return;
        const primary = findActivePrimaryBinding(shortcutSettings.bindings);
        if (primary?.modifier) {
          setCapturedBareModifier(primary.modifier);
          setHotkey("");
          setHoldToTalk(primary.action === "hold_to_record");
        } else {
          setHotkey("Alt+Space");
        }
        setHotkeyHydrated(true);
      })
      .catch((error) => {
        if (cancelled) return;
        log.error("[OnboardingDesktop] Failed to restore configured hotkey:", error);
        setHotkey("Alt+Space");
        setHotkeyHydrated(true);
      });
    return () => {
      cancelled = true;
    };
  }, [settings, hotkeyHydrated]);

  const confirmSource = (nextSourceType: SourceType) => {
    sourceChosenByUser.current = true;
    setSourceType(nextSourceType);
  };

  const checkSinglePermission = async (type: "microphone" | "accessibility") => {
    setCheckingPermissions((prev) => new Set(prev).add(type));

    try {
      if (type === "microphone") {
        await checkMicPermission();
      } else {
        await checkAccessPermission();
      }
    } catch (error) {
      log.error(`Failed to check ${type} permission:`, error);
    } finally {
      setCheckingPermissions((prev) => {
        const next = new Set(prev);
        next.delete(type);
        return next;
      });
    }
  };

  const requestPermission = async (type: "microphone" | "accessibility") => {
    setIsRequestingPermission(type);
    try {
      if (type === "microphone") {
        const granted = await requestMicPermission();
        if (!granted) {
          await invoke("open_microphone_settings");
        }
      } else {
        const granted = await requestAccessPermission();
        if (!granted) {
          await invoke("open_accessibility_settings");
        }
      }
    } catch (error) {
      log.error(`Failed to request ${type} permission:`, error);
    } finally {
      setIsRequestingPermission(null);
    }
  };

  const selectModel = async (modelName: string, source: "local" | "cloud") => {
    const info: ModelInfo | undefined = models[modelName];
    if (!info) {
      toast.error("That model is not available");
      return;
    }
    sourceChosenByUser.current = true;
    await invoke("set_active_remote_server", { serverId: null });
    setActiveRemoteServerId(null);
    setSourceType(source);
    await updateSettings({
      current_model: modelName,
      current_model_engine: info.engine ?? "whisper",
    });
  };

  const selectLocalModel = (modelName: string) => selectModel(modelName, "local");

  const selectCloudModel = (modelName: string) => selectModel(modelName, "cloud");

  const handleCloudKeySubmit = async (apiKey: string) => {
    if (!activeCloudProvider) return;
    setIsSavingCloudKey(true);
    try {
      await activeCloudProvider.addKey(apiKey);
      await loadModels();
      await selectCloudModel(activeCloudProvider.modelName);
      setCloudModelSetup(null);
      toast.success(`${activeCloudProvider.providerName} connected`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error(`Failed to connect ${activeCloudProvider.providerName}: ${message}`);
    } finally {
      setIsSavingCloudKey(false);
    }
  };

  const switchToLocalReadiness = () => {
    confirmSource("local");
  };

  const selectRemoteServer = async (serverId: string) => {
    const server = remoteServers.find((candidate) => candidate.id === serverId);
    if (!isRemoteServerOnline(server)) {
      toast.error("Remote Voicetypr is not online yet");
      return;
    }

    await invoke("set_active_remote_server", { serverId });
    setActiveRemoteServerId(serverId);
    confirmSource("remote");
    toast.success("Remote Voicetypr selected");
  };

  const handleRemoteServerAdded = (server: SavedConnection) => {
    setRemoteServers((prev) => {
      const existingIndex = prev.findIndex((candidate) => candidate.id === server.id);
      if (existingIndex >= 0) {
        const updated = [...prev];
        updated[existingIndex] = server;
        return updated;
      }
      return [...prev, server];
    });
    setDiscoveredRemoteServers((prev) =>
      prev.filter(
        (candidate) => !(candidate.host === server.host && candidate.port === server.port),
      ),
    );
    setActiveRemoteServerId(server.id);
    void invoke("set_active_remote_server", { serverId: server.id }).catch((error) => {
      log.error("[OnboardingDesktop] Failed to activate remote server:", error);
      toast.error("Remote Voicetypr was added, but could not be selected");
    });
    confirmSource("remote");
    void loadRemoteServers();
  };

  const handleAddDiscoveredRemoteServer = async (server: DiscoveredRemoteServer) => {
    if (server.auth_required) {
      setSelectedDiscoveredServer(server);
      setShowAddRemoteModal(true);
      return;
    }

    try {
      const added = await invoke<SavedConnection>("add_remote_server", {
        host: server.host,
        port: server.port,
        password: null,
        name: server.name,
      });
      handleRemoteServerAdded(added);
      toast.success(`${server.name} selected`);
    } catch (error) {
      log.error("[OnboardingDesktop] Failed to add discovered remote server:", error);
      toast.error(error instanceof Error ? error.message : "Failed to add remote Voicetypr");
    }
  };

  // Stable id for the onboarding-created HoldToRecord modifier_hold binding.
  // Using a fixed id means repeated saves replace the same entry instead of
  // accumulating stale bindings.
  const ONBOARDING_HOLD_ID = "onboarding-primary-hold";

  const saveHotkeySettings = async () => {
    if (capturedBareModifier) {
      // ── Bare modifier path ────────────────────────────────────────────
      // 1. Unregister (and clear) any existing primary global shortcut.
      //    set_global_shortcut("") is the backend's "clear primary" contract.
      await invoke("set_global_shortcut", { shortcut: "" });

      // 2. Upsert the native binding with the stable id, action determined by Hold to talk.
      //    holdToTalk ON  → modifier_hold / hold_to_record (PTT).
      //    holdToTalk OFF → isolated_tap / toggle_recording (single tap to toggle).
      const currentSettings = await invoke<ShortcutSettings>("get_shortcut_settings");
      const newBinding: ShortcutBinding = holdToTalk
        ? {
            id: ONBOARDING_HOLD_ID,
            action: "hold_to_record",
            shortcut: "",
            trigger: "hold",
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: "modifier_hold",
            modifier: {
              modifier: capturedBareModifier.modifier as ModifierKind,
              side: capturedBareModifier.side as ModifierSide,
            },
          }
        : {
            id: ONBOARDING_HOLD_ID,
            action: "toggle_recording",
            shortcut: "",
            trigger: "pressed",
            enabled: true,
            allow_risky_combo: false,
            trigger_kind: "isolated_tap",
            modifier: {
              modifier: capturedBareModifier.modifier as ModifierKind,
              side: capturedBareModifier.side as ModifierSide,
            },
          };
      await invoke("update_shortcut_settings", {
        settings: {
          bindings: [
            ...(currentSettings?.bindings ?? []).filter((b) => b.id !== ONBOARDING_HOLD_ID),
            newBinding,
          ],
        },
      });

      // 3. Persist the remaining settings.
      await updateSettings({
        hotkey: "",
        recording_mode: holdToTalk ? "push_to_talk" : "toggle",
        current_model: selectedModelName || "",
        current_model_engine: selectedModel?.engine ?? "whisper",
        speech_language: "en",
        onboarding_completed: false,
      });
    } else {
      // ── Combo / safe single-key path ──────────────────────────────────
      // 1. Register the new primary shortcut (also saves hotkey to store).
      await invoke("set_global_shortcut", { shortcut: hotkey });

      // 2. Remove any onboarding-created HoldToRecord binding so there is
      //    never BOTH a primary global shortcut and a modifier_hold binding.
      const currentSettings = await invoke<ShortcutSettings>("get_shortcut_settings");
      if ((currentSettings?.bindings ?? []).some((b) => b.id === ONBOARDING_HOLD_ID)) {
        await invoke("update_shortcut_settings", {
          settings: {
            bindings: (currentSettings?.bindings ?? []).filter((b) => b.id !== ONBOARDING_HOLD_ID),
          },
        });
      }

      // 3. Persist the remaining settings.
      await updateSettings({
        hotkey,
        recording_mode: holdToTalk ? "push_to_talk" : "toggle",
        current_model: selectedModelName || "",
        current_model_engine: selectedModel?.engine ?? "whisper",
        speech_language: "en",
        onboarding_completed: false,
      });
    }
  };

  const handleGpuToggle = async (checked: boolean) => {
    await updateSettings({ transcription_acceleration: checked ? "auto" : "cpu" });
  };

  const completeOnboarding = async () => {
    setIsSavingCompletion(true);
    onCompletionStart?.();
    try {
      // Persist privacy choices before publishing onboarding completion. The
      // settings context updates optimistically, so setting onboarding_completed
      // first would mount the main app's consent dialog while these writes were
      // still pending. That dialog would then retain the old default-on state.
      await invoke("set_telemetry_consent", { enabled: telemetryOptIn });
      await invoke("set_product_analytics_consent", {
        enabled: analyticsOptIn,
      });

      await updateSettings({ onboarding_completed: true }, { publishAfterSave: true });

      try {
        await invoke("record_onboarding_completed");
      } catch (analyticsError) {
        // Capturing the acknowledgement event is best-effort. The consent
        // choices themselves have already been persisted successfully.
        log.error("Failed to record onboarding completion:", analyticsError);
      }
      onComplete();
    } catch (error) {
      onCompletionError?.();
      log.error("Failed to complete onboarding:", error);
      toast.error("Failed to finish onboarding. Please try again.");
    } finally {
      setIsSavingCompletion(false);
    }
  };

  const handleNext = async () => {
    try {
      if (currentStep === "welcome") {
        setCurrentStep("source");
        return;
      }

      if (currentStep === "source") {
        setCurrentStep("readiness");
        return;
      }

      if (currentStep === "permissions") {
        setCurrentStep("hotkey");
        return;
      }

      if (currentStep === "readiness") {
        if (sourceType !== "remote") {
          await invoke("set_active_remote_server", { serverId: null });
          setActiveRemoteServerId(null);
        }
        setCurrentStep(isMacOS ? "permissions" : "hotkey");
        return;
      }

      if (currentStep === "hotkey") {
        await saveHotkeySettings();
        setCurrentStep("success");
        return;
      }
    } catch (error) {
      log.error("Failed to advance onboarding:", error);
      toast.error(error instanceof Error ? error.message : "Failed to continue onboarding");
    }
  };

  const handleBack = () => {
    const previousIndex = currentIndex - 1;
    if (previousIndex >= 0) {
      setCurrentStep(steps[previousIndex]);
    }
  };

  const canProceed = () => {
    switch (currentStep) {
      case "source":
        return true;
      case "permissions":
        if (!isMacOS) return true;
        return (
          permissions.microphone.status === "granted" &&
          permissions.accessibility.status === "granted"
        );
      case "readiness":
        return sourceReady;
      case "hotkey":
        return !isEditingHotkey;
      default:
        return true;
    }
  };

  const nextDisabled = !canProceed();

  return {
    currentStep,
    currentIndex,
    steps,
    sourceType,
    confirmSource,
    handleBack,
    handleNext,
    nextDisabled,
    permissions,
    checkingPermissions,
    isRequestingPermission,
    checkSinglePermission,
    requestPermission,
    local: {
      localModelNames,
      models,
      downloadProgress,
      verifyingModels,
      downloadErrors,
      currentModel: settings?.current_model,
      isLoading,
      hasDownloadedLocalModel,
      localReady,
      transcriptionAcceleration: settings?.transcription_acceleration,
      onDownload: (name: string) => {
        void downloadModel(name);
      },
      onSelectLocal: (name: string) => {
        void selectLocalModel(name);
      },
      onCancelDownload: (name: string) => {
        void cancelDownload(name);
      },
      onDelete: handleDeleteModel,
      onRepair: (name: string) => {
        void repairModel(name);
      },
      isModelReady,
      onGpuToggle: (checked: boolean) => {
        void handleGpuToggle(checked);
      },
    },
    cloud: {
      cloudModelNames,
      models,
      currentModel: settings?.current_model,
      isLoading,
      cloudModelSetup,
      isSavingCloudKey,
      isModelReady,
      onSelectCloud: (name: string) => {
        void selectCloudModel(name);
      },
      onSetCloudModelSetup: setCloudModelSetup,
      onCloudKeySubmit: (apiKey: string) => {
        void handleCloudKeySubmit(apiKey);
      },
    },
    remote: {
      remoteServers,
      discoveredRemoteServers,
      activeRemoteServerId,
      isLoadingRemoteServers,
      showAddRemoteModal,
      selectedDiscoveredServer,
      onLoadRemoteServers: () => {
        void loadRemoteServers();
      },
      onOpenAddServer: () => {
        setSelectedDiscoveredServer(null);
        setShowAddRemoteModal(true);
      },
      onAddDiscovered: (server: DiscoveredRemoteServer) => {
        void handleAddDiscoveredRemoteServer(server);
      },
      onSelectRemote: (serverId: string) => {
        void selectRemoteServer(serverId);
      },
      onSwitchToLocal: switchToLocalReadiness,
      onServerAdded: handleRemoteServerAdded,
      onAddModalOpenChange: (open: boolean) => {
        setShowAddRemoteModal(open);
        if (!open) {
          setSelectedDiscoveredServer(null);
        }
      },
    },
    hotkey,
    holdToTalk,
    capturedBareModifier,
    onHotkeyChange: (value: string) => {
      setHotkeyHydrated(true);
      setHotkey(value);
      setCapturedBareModifier(null);
    },
    onEditingChange: setIsEditingHotkey,
    onBareModifier: (spec: BareModifierSpec) => {
      setHotkeyHydrated(true);
      setCapturedBareModifier(spec);
      setHotkey("");
    },
    onHoldToTalkChange: setHoldToTalk,
    telemetryOptIn,
    analyticsOptIn,
    isSavingCompletion,
    onTelemetryChange: setTelemetryOptIn,
    onAnalyticsChange: setAnalyticsOptIn,
    completeOnboarding,
  };
}
