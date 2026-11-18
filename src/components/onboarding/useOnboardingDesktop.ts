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
import { useOfflinePreparation } from "@/components/onboarding/useOfflinePreparation";
import type { StartingPolishStyle } from "@/components/onboarding/PolishStep";

// Matches the backend default (FALLBACK_PRIMARY in commands/shortcuts.rs).
const defaultHotkey = () => (isMacOS ? "Alt+Space" : "Control+Space");

const log = createLogger("onboarding");
const NO_DOWNLOAD_ERRORS: Readonly<Record<string, string>> = Object.freeze({});

export function useOnboardingDesktop({
  onCompletionStart,
  onCompletionError,
  onComplete,
  modelManagement,
  previewStep,
  licenseStatus,
}: OnboardingDesktopProps) {
  const needsTrial = licenseStatus?.status !== "licensed" && licenseStatus?.status !== "trial";
  const { settings, updateSettings } = useSettings();
  const preparation = useOfflinePreparation(modelManagement);
  const {
    permissionStatus: microphonePermissionStatus,
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
    downloadErrors = NO_DOWNLOAD_ERRORS,
    cancelDownload,
    isLoading,
  } = modelManagement;

  const [currentStep, setCurrentStep] = useState<Step>(previewStep ?? "promise");
  // Sharing is opt-in: the box starts unticked unless the user already chose.
  const [telemetryOptIn, setTelemetryOptIn] = useState(false);
  const [accessibilitySkipped, setAccessibilitySkipped] = useState(false);
  const [sourceType, setSourceType] = useState<SourceType>(() =>
    isCloudEngine(settings?.current_model_engine ?? "") ? "cloud" : "local",
  );
  const [hotkey, setHotkey] = useState(() => settings?.hotkey || "");
  const [isEditingHotkey, setIsEditingHotkey] = useState(false);
  const [capturedBareModifier, setCapturedBareModifier] = useState<BareModifierSpec | null>(null);
  const [isRequestingPermission, setIsRequestingPermission] = useState<string | null>(null);
  const [accessibilityRequested, setAccessibilityRequested] = useState(false);
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

  // A relaunched onboarding keeps a choice the user already made.
  useEffect(() => {
    let cancelled = false;
    invoke<{ enabled: boolean; consent_required?: boolean } | null>("get_telemetry_status")
      .then((telemetry) => {
        if (cancelled || !telemetry || telemetry.consent_required !== false) return;
        setTelemetryOptIn(telemetry.enabled);
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
      status: microphonePermissionStatus,
    } as PermissionState,
    accessibility: {
      status:
        hasAccessPermission === null ? "checking" : hasAccessPermission ? "granted"
          : accessibilityRequested ? "denied" : "not_determined",
    } as PermissionState,
  };

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
    (name: string) => models[name]?.downloaded === true && !models[name]?.requires_setup && !verifyingModels.has(name),
    [models, verifyingModels],
  );
  const localReady = Boolean(
    sourceType === "local" &&
    selectedModelName &&
    selectedModel &&
    isLocalModel(selectedModel) &&
    settings?.current_model_engine === selectedModel.engine &&
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
    if (currentStep !== "try" && currentStep !== "setup" && currentStep !== "done") return;
    void checkPermissions();
  }, [currentStep, checkPermissions]);

  useEffect(() => {
    if (currentStep !== "try" && currentStep !== "setup") return;
    const handleFocus = () => {
      void checkPermissionsRef.current();
    };

    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, [currentStep]);

  useEffect(() => {
    if (sourceType !== "remote") return;
    void (async () => {
      await Promise.resolve();
      await loadRemoteServers();
    })();
  }, [sourceType, loadRemoteServers]);

  const [sourceRestored, setSourceRestored] = useState(false);
  useEffect(() => {
    let cancelled = false;
    void invoke<string | null>("get_active_remote_server")
      .finally(() => {
        if (!cancelled) setSourceRestored(true);
      })
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
          setHotkey(defaultHotkey());
        }
        setHotkeyHydrated(true);
      })
      .catch((error) => {
        if (cancelled) return;
        log.error("[OnboardingDesktop] Failed to restore configured hotkey:", error);
        setHotkey(defaultHotkey());
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
        if (microphonePermissionStatus === "not_determined") await requestMicPermission();
        else await invoke("open_microphone_settings");
      } else {
        if (!accessibilityRequested) {
          setAccessibilityRequested(true);
          await requestAccessPermission();
        } else await invoke("open_accessibility_settings");
      }
    } catch (error) {
      log.error(`Failed to request ${type} permission:`, error);
    } finally {
      setIsRequestingPermission(null);
    }
  };

  // Asks on first use; afterwards only System Settings can change the answer.
  const ensureMicrophone = async () => {
    if (microphonePermissionStatus === "not_determined") return requestMicPermission();
    await requestPermission("microphone");
    return false;
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

  const prepareLocal = async () => {
    if (!preparation.recommended) return false;
    await invoke("set_active_remote_server", { serverId: null });
    setActiveRemoteServerId(null);
    await preparation.prepare(preparation.recommended);
    return true;
  };

  // One automatic start per model, so a cancelled download stays cancelled
  // while a model chosen for another language still starts.
  const preparingStarted = useRef<string | null>(null);
  const startPreparing = () => {
    const target = preparation.recommended;
    if (!target || preparingStarted.current === target || sourceReady || models[target]?.downloaded) return;
    preparingStarted.current = target;
    void prepareLocal().catch(() => {
      preparingStarted.current = null;
    });
  };

  const startPreparingRef = useRef(startPreparing);
  useEffect(() => {
    startPreparingRef.current = startPreparing;
  });
  // The on-device engine starts downloading on the source step, where its progress shows.
  useEffect(() => {
    if (currentStep === "source" && sourceType === "local" && sourceRestored) startPreparingRef.current();
  }, [currentStep, sourceType, sourceRestored, preparation.recommended]);

  const chooseStartingStyle = async (style: StartingPolishStyle | null, polishOn: boolean) => {
    if (style && polishOn) await invoke("update_enhancement_options", { options: { preset: style } });
    else await updateSettings({ starting_polish_style: style });
    setCurrentStep("try");
  };

  const afterPromise: Step = needsTrial ? "trial" : "source";
  // Reached from the first try: Continue goes straight back there.
  const [sourceFromTry, setSourceFromTry] = useState(false);

  const handleNext = async () => {
    try {
      if (currentStep === "promise") {
        setCurrentStep(afterPromise);
        return;
      }

      if (currentStep === "trial") {
        setCurrentStep("source");
        return;
      }

      if (currentStep === "source") {
        if (preparation.savingLanguage) return;
        if (sourceType === "local" && !sourceReady) {
          if (models[preparation.recommended ?? ""]?.downloaded) await prepareLocal();
          else startPreparing();
        }
        await leaveSource();
        setCurrentStep(sourceFromTry ? "try" : "setup");
        setSourceFromTry(false);
        return;
      }

      if (currentStep === "setup") {
        await saveHotkeySettings();
        setCurrentStep("polish");
        return;
      }

      if (currentStep === "try") {
        setCurrentStep("done");
        return;
      }
    } catch (error) {
      log.error("Failed to advance onboarding:", error);
      toast.error(error instanceof Error ? error.message : "Failed to continue onboarding");
    }
  };

  // Leaving the source step either way: a remote server stays active only when
  // it is the chosen source, so "this Mac" never sends audio to the network.
  const leaveSource = async () => {
    if (sourceType !== "remote" && activeRemoteServerId) {
      await invoke("set_active_remote_server", { serverId: null });
      setActiveRemoteServerId(null);
    }
  };

  const handleBack = async () => {
    const previous: Partial<Record<Step, Step>> = {
      trial: "promise",
      source: sourceFromTry ? "try" : needsTrial ? "trial" : "promise",
      setup: "source",
      polish: "setup",
      try: "polish",
      done: "try",
    };
    const step = previous[currentStep];
    if (currentStep === "source") {
      try {
        await leaveSource();
      } catch (error) {
        log.error("Failed to leave the source step:", error);
        toast.error("Couldn’t switch where dictation runs. Try again.");
        return;
      }
      setSourceFromTry(false);
    }
    if (step) setCurrentStep(step);
  };

  const canProceed = () => {
    switch (currentStep) {
      case "source":
        return !preparation.savingLanguage;
      case "setup":
        return !isEditingHotkey && (!isMacOS || accessibilitySkipped ||
          permissions.accessibility.status === "granted");
      default:
        return true;
    }
  };

  const nextDisabled = !canProceed();

  return {
    currentStep,
    chooseStartingStyle,
    isEditingHotkey,
    goTo: setCurrentStep,
    sourceType,
    preparation,
    sourceReady,
    confirmSource,
    chooseSourceFromTry: () => {
      setSourceFromTry(true);
      setCurrentStep("source");
    },
    skipSetup: () => setCurrentStep("polish"),
    handleBack,
    handleNext,
    nextDisabled,
    permissions,
    checkingPermissions,
    isRequestingPermission,
    checkSinglePermission,
    requestPermission,
    ensureMicrophone,
    local: {
      localModelNames,
      recommendedModel: preparation.bestMatch,
      models,
      downloadProgress,
      verifyingModels,
      downloadErrors,
      currentModel: settings?.current_model,
      isLoading,
      hasDownloadedLocalModel,
      localReady,
      transcriptionAcceleration: settings?.transcription_acceleration,
      onDownload: preparation.download,
      onSelectLocal: (name: string) => {
        confirmSource("local");
        void preparation.prepare(name);
      },
      onCancelDownload: (name: string) => {
        void cancelDownload(name);
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
    accessibilitySkipped,
    skipAccessibility: () => setAccessibilitySkipped(true),
    saveHotkeySettings,
    telemetryOptIn,
    isSavingCompletion,
    onTelemetryChange: setTelemetryOptIn,
    completeOnboarding,
  };
}
