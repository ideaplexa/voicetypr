import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ModelInfo } from "@/types";
import { OnboardingDesktop } from "./OnboardingDesktop";

const {
  invokeMock,
  updateSettingsMock,
  onCompleteMock,
  onCompletionStartMock,
  onCompletionErrorMock,
  eventListeners,
  modelManagement,
  settingsState,
  settingsView,
} = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  updateSettingsMock: vi.fn(),
  onCompleteMock: vi.fn(),
  onCompletionStartMock: vi.fn(),
  onCompletionErrorMock: vi.fn(),
  eventListeners: new Map<string, Set<(event: { payload: unknown }) => void>>(),
  settingsState: {
    hotkey: "CommandOrControl+Shift+Space",
    current_model: "base.en",
    current_model_engine: "whisper",
    speech_language: "en",
    onboarding_completed: false,
  },
  settingsView: { current: null as null | Record<string, unknown> },
  modelManagement: {
    models: {
      "base.en": {
        name: "base.en",
        display_name: "Base English",
        size: 74,
        url: "",
        sha256: "",
        downloaded: true,
        speed_score: 7,
        accuracy_score: 5,
        recommended: false,
        engine: "whisper",
        kind: "local",
        requires_setup: false,
      },
    } as Record<string, ModelInfo>,
    modelOrder: ["base.en"],
    downloadProgress: {} as Record<string, number>,
    verifyingModels: new Set<string>(),
    loadModels: vi.fn(),
    downloadModel: vi.fn(),
    cancelDownload: vi.fn(),
    deleteModel: vi.fn(),
    sortedModels: [],
    isLoading: false,
  },
}));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: settingsView.current,
    updateSettings: updateSettingsMock,
  }),
}));

vi.mock("@/hooks/useMicrophonePermission", () => ({
  useMicrophonePermission: () => ({
    hasPermission: true,
    checkPermission: vi.fn().mockResolvedValue(true),
    requestPermission: vi.fn().mockResolvedValue(true),
  }),
}));

vi.mock("@/hooks/useAccessibilityPermission", () => ({
  useAccessibilityPermission: () => ({
    hasPermission: true,
    checkPermission: vi.fn().mockResolvedValue(true),
    requestPermission: vi.fn().mockResolvedValue(true),
  }),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: (event: { payload: unknown }) => void) => {
    const handlers = eventListeners.get(event) ?? new Set();
    handlers.add(handler);
    eventListeners.set(event, handlers);
    return Promise.resolve(() => handlers.delete(handler));
  }),
  emit: vi.fn().mockResolvedValue(undefined),
}));

const platformMock = vi.hoisted(() => ({ isMacOS: true, isWindows: false, isLinux: false }));
vi.mock("@/lib/platform", () => platformMock);

const renderOnboarding = () =>
  render(
    <OnboardingDesktop
      onCompletionStart={onCompletionStartMock}
      onCompletionError={onCompletionErrorMock}
      onComplete={onCompleteMock}
      modelManagement={modelManagement as never}
    />,
  );

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(platformMock, { isMacOS: true, isWindows: false, isLinux: false });
  eventListeners.clear();
  Object.assign(settingsState, {
    hotkey: "CommandOrControl+Shift+Space",
    current_model: "base.en",
    current_model_engine: "whisper",
    speech_language: "en",
    onboarding_completed: false,
  });
  settingsView.current = settingsState;
  delete (settingsState as Record<string, unknown>).transcription_acceleration;
  delete (settingsState as Record<string, unknown>).recording_mode;
  modelManagement.models = {
    "base.en": {
      name: "base.en",
      display_name: "Base English",
      size: 74,
      url: "",
      sha256: "",
      downloaded: true,
      speed_score: 7,
      accuracy_score: 5,
      recommended: false,
      engine: "whisper",
      kind: "local",
      requires_setup: false,
    },
  };
  modelManagement.loadModels.mockReset();
  modelManagement.loadModels.mockResolvedValue(undefined);
  modelManagement.modelOrder = ["base.en"];
  modelManagement.downloadProgress = {};
  updateSettingsMock.mockImplementation((updates: Partial<typeof settingsState>) => {
    Object.assign(settingsState, updates);
    return Promise.resolve();
  });
  invokeMock.mockImplementation((command: string) => {
    switch (command) {
      case "discover_remote_servers":
        return Promise.resolve([]);
      case "list_remote_servers":
        return Promise.resolve([]);
      case "get_active_remote_server":
        return Promise.resolve(null);
      case "set_active_remote_server":
      case "set_global_shortcut":
        return Promise.resolve(true);
      case "get_audio_devices":
        return Promise.resolve([]);
      default:
        return Promise.resolve(null);
    }
  });
});

describe("OnboardingDesktop", () => {
  it("saves the hotkey and reaches success without a sample transcription", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await screen.findByRole("heading", { name: /try it now/i });
    expect(screen.queryByText(/do your first transcription/i)).not.toBeInTheDocument();
    expect(eventListeners.has("transcription-added")).toBe(false);
    expect(invokeMock).toHaveBeenCalledWith("set_global_shortcut", {
      shortcut: "CommandOrControl+Shift+Space",
    });

    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    expect(updateSettingsMock).toHaveBeenCalledWith(
      { onboarding_completed: true },
      { publishAfterSave: true },
    );
    expect(onCompleteMock).toHaveBeenCalledTimes(1);
    expect(onCompleteMock).toHaveBeenCalledWith();
  });

  it("strips a stale onboarding hold binding when a combo hotkey is saved", async () => {
    const user = userEvent.setup();
    const userBinding = {
      id: "user-custom",
      action: "copy_last_transcription",
      shortcut: "CommandOrControl+Shift+C",
      trigger: "pressed",
      enabled: true,
      allow_risky_combo: false,
      trigger_kind: "combo",
      modifier: null,
    };
    const onboardingHold = {
      id: "onboarding-primary-hold",
      action: "hold_to_record",
      shortcut: "",
      trigger: "hold",
      enabled: true,
      allow_risky_combo: false,
      trigger_kind: "modifier_hold",
      modifier: { modifier: "alt", side: "right" },
    };
    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "discover_remote_servers":
        case "list_remote_servers":
          return Promise.resolve([]);
        case "get_active_remote_server":
          return Promise.resolve(null);
        case "set_active_remote_server":
        case "set_global_shortcut":
          return Promise.resolve(true);
        case "get_shortcut_settings":
          return Promise.resolve({ bindings: [userBinding, onboardingHold] });
        case "get_audio_devices":
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await screen.findByRole("heading", { name: /try it now/i });

    // Combo save registers the primary global shortcut AND removes only the
    // onboarding-created hold binding, so recording can never fire from both a
    // global shortcut and a modifier_hold trigger at once.
    expect(invokeMock).toHaveBeenCalledWith("set_global_shortcut", {
      shortcut: "CommandOrControl+Shift+Space",
    });
    expect(invokeMock).toHaveBeenCalledWith("update_shortcut_settings", {
      settings: { bindings: [userBinding] },
    });
  });

  it("resyncs the hotkey when settings arrive after mount", async () => {
    const user = userEvent.setup();
    settingsView.current = null;
    const view = renderOnboarding();

    settingsState.hotkey = "CommandOrControl+Alt+M";
    settingsView.current = settingsState;
    view.rerender(
      <OnboardingDesktop
        onCompletionStart={onCompletionStartMock}
        onCompletionError={onCompletionErrorMock}
        onComplete={onCompleteMock}
        modelManagement={modelManagement as never}
      />,
    );

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    expect(invokeMock).toHaveBeenCalledWith("set_global_shortcut", {
      shortcut: "CommandOrControl+Alt+M",
    });
  });

  it("keeps a user-edited hotkey when settings arrive later", async () => {
    const user = userEvent.setup();
    settingsView.current = null;
    const server = {
      id: "remote-1",
      name: "Studio Mac",
      host: "10.0.0.12",
      port: 47842,
      created_at: 1,
      model: "base.en",
      status: "Online",
    };
    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "discover_remote_servers":
          return Promise.resolve([]);
        case "list_remote_servers":
          return Promise.resolve([server]);
        case "get_active_remote_server":
          return Promise.resolve(null);
        case "check_remote_server_status":
          return Promise.resolve(server);
        case "set_active_remote_server":
        case "set_global_shortcut":
          return Promise.resolve(true);
        case "get_audio_devices":
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });
    const view = renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /another computer/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(await screen.findByRole("button", { name: /use this server/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await user.click(screen.getByRole("button", { name: /continue/i }));

    await user.click(await screen.findByRole("button", { name: /^Change shortcut$/i }));
    await user.click(screen.getByTitle("Change hotkey"));
    fireEvent.keyDown(window, { key: "k", code: "KeyK", metaKey: true });
    fireEvent.keyUp(window, { key: "k", code: "KeyK", metaKey: true });
    await user.click(screen.getByTitle("Save hotkey"));

    settingsState.hotkey = "CommandOrControl+Shift+P";
    settingsView.current = settingsState;
    view.rerender(
      <OnboardingDesktop
        onCompletionStart={onCompletionStartMock}
        onCompletionError={onCompletionErrorMock}
        onComplete={onCompleteMock}
        modelManagement={modelManagement as never}
      />,
    );

    await user.click(screen.getByRole("button", { name: /save hotkey/i }));
    expect(invokeMock).toHaveBeenCalledWith("set_global_shortcut", {
      shortcut: "CommandOrControl+K",
    });
  });

  it("restores an existing bare-modifier hotkey when onboarding is rerun", async () => {
    const user = userEvent.setup();
    settingsState.hotkey = "";
    const existingHold = {
      id: "onboarding-primary-hold",
      action: "hold_to_record",
      shortcut: "",
      trigger: "hold",
      enabled: true,
      allow_risky_combo: false,
      trigger_kind: "modifier_hold",
      modifier: { modifier: "alt", side: "right" },
    };
    invokeMock.mockImplementation((command: string) => {
      switch (command) {
        case "discover_remote_servers":
        case "list_remote_servers":
          return Promise.resolve([]);
        case "get_active_remote_server":
          return Promise.resolve(null);
        case "get_shortcut_settings":
          return Promise.resolve({ bindings: [existingHold] });
        case "set_active_remote_server":
        case "set_global_shortcut":
        case "update_shortcut_settings":
          return Promise.resolve(true);
        case "get_audio_devices":
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });

    renderOnboarding();
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: { bindings: [existingHold] },
      });
    });
    expect(invokeMock).not.toHaveBeenCalledWith("set_global_shortcut", {
      shortcut: "Alt+Space",
    });
  });

  it("keeps the selected local model when onboarding is rerun", async () => {
    const user = userEvent.setup();

    renderOnboarding();
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    expect(
      await screen.findByRole("heading", { name: /where should your voice become text/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
    expect(settingsState.current_model).toBe("base.en");
    expect(updateSettingsMock).not.toHaveBeenCalledWith(
      expect.objectContaining({ current_model: expect.any(String) }),
    );
  });

  it("shows local, cloud, and remote as explicit source choices", async () => {
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });

    expect(
      screen.getByRole("heading", { name: /where should your voice become text/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /on this Mac/i })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: /^Cloud$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /another computer/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled();
    expect(updateSettingsMock).not.toHaveBeenCalledWith(
      expect.objectContaining({
        current_model: "base.en",
      }),
    );
  });

  it("does not overwrite an explicit source choice when remote restore resolves late", async () => {
    const user = userEvent.setup();
    let resolveActiveRemote!: (serverId: string | null) => void;
    const activeRemote = new Promise<string | null>((resolve) => {
      resolveActiveRemote = resolve;
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_audio_devices") return Promise.resolve([]);
      if (command === "get_active_remote_server") return activeRemote;
      if (command === "discover_remote_servers" || command === "list_remote_servers") {
        return Promise.resolve([]);
      }
      return Promise.resolve(null);
    });

    renderOnboarding();
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /^Cloud$/i }));

    await act(async () => {
      resolveActiveRemote("remote-1");
      await activeRemote;
    });

    expect(screen.getByRole("button", { name: /^Cloud$/i })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: /another computer/i })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("does not overwrite a selected model when remote restore resolves late", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    let resolveActiveRemote!: (serverId: string | null) => void;
    const activeRemote = new Promise<string | null>((resolve) => {
      resolveActiveRemote = resolve;
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_audio_devices") return Promise.resolve([]);
      if (command === "get_active_remote_server") return activeRemote;
      if (command === "discover_remote_servers" || command === "list_remote_servers") {
        return Promise.resolve([]);
      }
      return Promise.resolve(null);
    });

    renderOnboarding();
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(await screen.findByText("Base English"));

    await act(async () => {
      resolveActiveRemote("remote-1");
      await activeRemote;
    });

    expect(
      screen.getByRole("heading", { name: /where should your voice become text/i }),
    ).toBeInTheDocument();
    expect(updateSettingsMock).toHaveBeenCalledWith({
      current_model: "base.en",
      current_model_engine: "whisper",
    });
  });

  it("guides users to select a downloaded local model before continuing", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));

    expect(screen.getByText(/^Select a downloaded model$/)).toBeInTheDocument();
    expect(screen.getByText(/onboarding needs one selected/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /continue/i })).toBeDisabled();
  });

  it("lets remote-first users switch to local setup from the empty remote state", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /another computer/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await user.click(await screen.findByRole("button", { name: /choose local instead/i }));

    expect(
      screen.getByRole("heading", { name: /where should your voice become text/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/^Select a downloaded model$/)).toBeInTheDocument();
  });

  it("allows an online remote Voicetypr source without a local model", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    modelManagement.models = {} as Record<string, ModelInfo>;
    modelManagement.modelOrder = [];

    invokeMock.mockImplementation((command: string, args?: { serverId?: string }) => {
      switch (command) {
        case "discover_remote_servers":
          return Promise.resolve([]);
        case "list_remote_servers":
          return Promise.resolve([
            {
              id: "remote-1",
              name: "Studio Mac",
              host: "10.0.0.12",
              port: 47842,
              created_at: 1,
              model: "parakeet-tdt-0.6b-v2",
              status: "Online",
            },
          ]);
        case "get_active_remote_server":
          return Promise.resolve(null);
        case "check_remote_server_status":
          return Promise.resolve({
            id: args?.serverId ?? "remote-1",
            name: "Studio Mac",
            host: "10.0.0.12",
            port: 47842,
            created_at: 1,
            model: "parakeet-tdt-0.6b-v2",
            status: "Online",
          });
        case "set_active_remote_server":
        case "set_global_shortcut":
          return Promise.resolve(true);
        case "get_audio_devices":
          return Promise.resolve([]);
        default:
          return Promise.resolve(null);
      }
    });

    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /another computer/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    const useServerButton = await screen.findByRole("button", {
      name: /use this server/i,
    });
    await user.click(useServerButton);

    expect(invokeMock).toHaveBeenCalledWith("set_active_remote_server", {
      serverId: "remote-1",
    });
    expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled();
  });

  it("connects a cloud provider during onboarding", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    settingsState.current_model_engine = "whisper";
    modelManagement.models = {
      soniox: {
        name: "soniox",
        display_name: "Soniox",
        size: 0,
        url: "",
        sha256: "",
        downloaded: false,
        speed_score: 9,
        accuracy_score: 9,
        recommended: false,
        engine: "soniox",
        kind: "cloud",
        requires_setup: true,
      },
    };
    modelManagement.modelOrder = ["soniox"];
    modelManagement.loadModels.mockImplementation(async () => {
      modelManagement.models.soniox.downloaded = true;
      modelManagement.models.soniox.requires_setup = false;
    });

    renderOnboarding();
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /^Cloud$/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    expect(
      screen.getByRole("heading", { name: /where should your voice become text/i }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /add api key/i }));
    await user.type(screen.getByPlaceholderText("Enter your Soniox API key"), "test-cloud-key");
    await user.click(screen.getByRole("button", { name: /save api key/i }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("validate_stt_key", {
        provider: "soniox",
        apiKey: "test-cloud-key",
      });
      expect(updateSettingsMock).toHaveBeenCalledWith({
        current_model: "soniox",
        current_model_engine: "soniox",
      });
    });
    expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled();
  });

  it("Windows GPU toggle ON→OFF persists 'cpu' (default state: acceleration undefined → switch ON)", async () => {
    platformMock.isMacOS = false;
    platformMock.isWindows = true;
    // settingsState.transcription_acceleration is undefined → switch resolves to ON
    const user = userEvent.setup();
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));

    const gpuSwitch = screen.getByRole("switch", { name: /use gpu acceleration/i });
    expect(gpuSwitch).toBeInTheDocument();
    expect(gpuSwitch).toHaveAttribute("aria-checked", "true");

    await user.click(gpuSwitch);
    expect(updateSettingsMock).toHaveBeenCalledWith({ transcription_acceleration: "cpu" });
  });

  it("Windows GPU toggle OFF→ON persists 'auto' (prior state: acceleration 'cpu' → switch OFF)", async () => {
    platformMock.isMacOS = false;
    platformMock.isWindows = true;
    (settingsState as Record<string, unknown>).transcription_acceleration = "cpu";
    const user = userEvent.setup();
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));

    const gpuSwitch = screen.getByRole("switch", { name: /use gpu acceleration/i });
    expect(gpuSwitch).toHaveAttribute("aria-checked", "false");

    await user.click(gpuSwitch);
    expect(updateSettingsMock).toHaveBeenCalledWith({ transcription_acceleration: "auto" });
  });

  it("does not show the GPU toggle on macOS", async () => {
    // platformMock defaults to isMacOS:true, isWindows:false (reset by beforeEach)
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate through macOS flow to readiness (welcome→source→permissions→readiness)
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));

    expect(screen.queryByText(/use gpu acceleration/i)).not.toBeInTheDocument();
  });

  it("saves isolated_tap binding when bare modifier captured with Hold to talk OFF", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate to hotkey step (macOS: welcome→source→permissions→readiness→hotkey)
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    // Enter HotkeyInput edit mode
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await user.click(await screen.findByRole("button", { name: /^Change shortcut$/i }));
    await user.click(screen.getByTitle("Change hotkey"));

    // Simulate Right Alt bare modifier press + release; waitFor ensures the
    // keydown listener is attached and pendingBareModifier is set before clicking Save.
    fireEvent.keyDown(window, { key: "Alt", code: "AltRight", altKey: true });
    fireEvent.keyUp(window, { key: "Alt", code: "AltRight" });
    await waitFor(() => expect(screen.getByTitle("Save hotkey")).not.toBeDisabled());

    // Save within HotkeyInput (the checkmark icon button)
    await user.click(screen.getByTitle("Save hotkey"));

    // Hold to talk is OFF by default; save the step
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /try it now/i });

    // isolated_tap / toggle_recording / pressed
    expect(invokeMock).toHaveBeenCalledWith("update_shortcut_settings", {
      settings: {
        bindings: [
          expect.objectContaining({
            id: "onboarding-primary-hold",
            trigger_kind: "isolated_tap",
            action: "toggle_recording",
            trigger: "pressed",
            modifier: { modifier: "alt", side: "right" },
            shortcut: "",
          }),
        ],
      },
    });
    expect(updateSettingsMock).toHaveBeenCalledWith(
      expect.objectContaining({ recording_mode: "toggle" }),
    );
  });

  it("saves modifier_hold binding when bare modifier captured with Hold to talk ON", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate to hotkey step
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    // Enter HotkeyInput edit mode
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await user.click(await screen.findByRole("button", { name: /^Change shortcut$/i }));
    await user.click(screen.getByTitle("Change hotkey"));

    // Simulate Right Alt bare modifier press + release
    fireEvent.keyDown(window, { key: "Alt", code: "AltRight", altKey: true });
    fireEvent.keyUp(window, { key: "Alt", code: "AltRight" });
    await waitFor(() => expect(screen.getByTitle("Save hotkey")).not.toBeDisabled());

    // Save within HotkeyInput
    await user.click(screen.getByTitle("Save hotkey"));

    // Toggle "Hold to talk" ON
    await user.click(screen.getByRole("switch", { name: /hold to talk/i }));

    // Save the step
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("heading", { name: /try it now/i });

    // modifier_hold / hold_to_record / hold
    expect(invokeMock).toHaveBeenCalledWith("update_shortcut_settings", {
      settings: {
        bindings: [
          expect.objectContaining({
            id: "onboarding-primary-hold",
            trigger_kind: "modifier_hold",
            action: "hold_to_record",
            trigger: "hold",
            modifier: { modifier: "alt", side: "right" },
            shortcut: "",
          }),
        ],
      },
    });
    expect(updateSettingsMock).toHaveBeenCalledWith(
      expect.objectContaining({ recording_mode: "push_to_talk" }),
    );
  });

  it("defaults both privacy choices on and persists them on completion", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate to the success step (welcome→source→permissions→readiness→hotkey).
    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await screen.findByRole("heading", { name: /try it now/i });

    expect(screen.getByRole("checkbox", { name: /crash & error reporting/i })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /usage analytics/i })).toBeChecked();

    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: true });
      expect(invokeMock).toHaveBeenCalledWith("set_product_analytics_consent", {
        enabled: true,
      });
      expect(invokeMock).toHaveBeenCalledWith("record_onboarding_completed");
    });
  });

  it("persists an analytics opt-out independently during onboarding", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));

    await screen.findByRole("heading", { name: /try it now/i });

    await user.click(screen.getByRole("checkbox", { name: /usage analytics/i }));
    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: true });
      expect(invokeMock).toHaveBeenCalledWith("set_product_analytics_consent", {
        enabled: false,
      });
    });
  });

  it("keeps onboarding active until both opt-outs are persisted", async () => {
    const user = userEvent.setup();
    let resolveAnalytics: (() => void) | undefined;
    const analyticsPersisted = new Promise<void>((resolve) => {
      resolveAnalytics = resolve;
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_audio_devices") return Promise.resolve([]);
      if (command === "set_product_analytics_consent") {
        return analyticsPersisted;
      }
      if (command === "discover_remote_servers" || command === "list_remote_servers") {
        return Promise.resolve([]);
      }
      if (command === "get_active_remote_server") {
        return Promise.resolve(null);
      }
      return Promise.resolve(null);
    });
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("checkbox", { name: /crash & error reporting/i }));
    await user.click(screen.getByRole("checkbox", { name: /usage analytics/i }));
    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: false });
      expect(invokeMock).toHaveBeenCalledWith("set_product_analytics_consent", {
        enabled: false,
      });
    });
    expect(onCompletionStartMock).toHaveBeenCalledTimes(1);
    expect(
      updateSettingsMock.mock.calls.some(([updates]) => updates?.onboarding_completed === true),
    ).toBe(false);
    expect(onCompleteMock).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: /start using voicetypr/i })).toBeDisabled();

    resolveAnalytics?.();

    await waitFor(() => {
      expect(updateSettingsMock).toHaveBeenCalledWith(
        { onboarding_completed: true },
        { publishAfterSave: true },
      );
      expect(onCompleteMock).toHaveBeenCalledTimes(1);
    });
  });

  it("keeps onboarding recoverable when analytics consent fails after diagnostics saves", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_audio_devices") return Promise.resolve([]);
      if (command === "set_product_analytics_consent") {
        return Promise.reject(new Error("analytics consent store unavailable"));
      }
      if (command === "discover_remote_servers" || command === "list_remote_servers") {
        return Promise.resolve([]);
      }
      if (command === "get_active_remote_server") {
        return Promise.resolve(null);
      }
      return Promise.resolve(null);
    });
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("checkbox", { name: /crash & error reporting/i }));
    await user.click(screen.getByRole("checkbox", { name: /usage analytics/i }));
    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => {
      expect(onCompletionErrorMock).toHaveBeenCalledTimes(1);
    });
    expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: false });
    expect(invokeMock).not.toHaveBeenCalledWith("record_onboarding_completed");
    expect(
      updateSettingsMock.mock.calls.some(([updates]) => updates?.onboarding_completed === true),
    ).toBe(false);
    expect(onCompleteMock).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: /try it now/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /start using voicetypr/i })).toBeEnabled();

    invokeMock.mockResolvedValue(null);
    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => {
      expect(
        invokeMock.mock.calls.filter(([command]) => command === "set_product_analytics_consent"),
      ).toHaveLength(2);
      expect(updateSettingsMock).toHaveBeenCalledWith(
        { onboarding_completed: true },
        { publishAfterSave: true },
      );
      expect(onCompleteMock).toHaveBeenCalledTimes(1);
    });
    expect(onCompletionStartMock).toHaveBeenCalledTimes(2);
    expect(onCompletionErrorMock).toHaveBeenCalledTimes(1);
  });

  it("keeps opt-outs selected when the completion save fails and succeeds on retry", async () => {
    const user = userEvent.setup();
    let completionAttempts = 0;
    updateSettingsMock.mockImplementation(
      (updates: Partial<typeof settingsState>, options?: { publishAfterSave?: boolean }) => {
        if (updates.onboarding_completed === true) {
          expect(options).toEqual({ publishAfterSave: true });
          completionAttempts += 1;
          if (completionAttempts === 1) {
            return Promise.reject(new Error("settings store unavailable"));
          }
        }
        Object.assign(settingsState, updates);
        return Promise.resolve();
      },
    );
    renderOnboarding();

    await screen.findByRole("heading", { name: /where should your voice become text/i });
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("checkbox", { name: /crash & error reporting/i }));
    await user.click(screen.getByRole("checkbox", { name: /usage analytics/i }));
    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => expect(onCompletionErrorMock).toHaveBeenCalledTimes(1));
    expect(settingsState.onboarding_completed).toBe(false);
    expect(onCompleteMock).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: /try it now/i })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /crash & error reporting/i })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /usage analytics/i })).not.toBeChecked();
    expect(screen.getByRole("button", { name: /start using voicetypr/i })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => {
      expect(settingsState.onboarding_completed).toBe(true);
      expect(onCompleteMock).toHaveBeenCalledTimes(1);
    });
    expect(completionAttempts).toBe(2);
    expect(onCompletionStartMock).toHaveBeenCalledTimes(2);
    expect(onCompletionErrorMock).toHaveBeenCalledTimes(1);
  });
  it.each(["macos", "windows"] as const)(
    "renders all three real phases on %s",
    async (platform) => {
      Object.assign(platformMock, {
        isMacOS: platform === "macos",
        isWindows: platform === "windows",
      });
      const user = userEvent.setup();
      renderOnboarding();
      expect(
        await screen.findByRole("button", {
          name: platform === "macos" ? "On this Mac" : "On this PC",
        }),
      ).toHaveAttribute("aria-pressed", "true");
      await user.click(screen.getByRole("button", { name: /continue/i }));
      expect(screen.getByText("Base English")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /continue/i }));
      expect(
        screen.getByRole("heading", {
          name:
            platform === "macos"
              ? "Let Voicetypr hear you and type for you"
              : "Check your microphone",
        }),
      ).toBeInTheDocument();
      expect(screen.getByRole("combobox")).toBeInTheDocument();
      expect(screen.getByText("You'll test it in the next step.")).toBeInTheDocument();
      expect(screen.queryByRole("meter")).not.toBeInTheDocument();
      if (platform === "macos")
        expect(screen.getByRole("heading", { name: "Accessibility" })).toBeInTheDocument();
      else expect(screen.queryByText("Accessibility")).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /continue/i }));
      expect(await screen.findByRole("textbox", { name: "Try a dictation" })).toHaveFocus();
      expect(screen.getByRole("button", { name: "Change shortcut" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Start using Voicetypr" }));
      await waitFor(() => expect(settingsState.onboarding_completed).toBe(true));
    },
  );

  it.each(["macos", "windows"] as const)(
    "shows the real model size and reachable download progress on %s",
    async (platform) => {
      Object.assign(platformMock, {
        isMacOS: platform === "macos",
        isWindows: platform === "windows",
      });
      const user = userEvent.setup();
      modelManagement.models["base.en"] = {
        ...modelManagement.models["base.en"],
        display_name: "Parakeet",
        size: 670 * 1024 * 1024,
        downloaded: false,
      };
      const view = renderOnboarding();
      await user.click(
        await screen.findByRole("button", { name: "Continue — download Parakeet (670 MB)" }),
      );
      expect(modelManagement.downloadModel).toHaveBeenCalledWith("base.en");
      modelManagement.downloadProgress = { "base.en": 42 };
      view.rerender(
        <OnboardingDesktop
          onComplete={onCompleteMock}
          modelManagement={modelManagement as never}
        />,
      );
      expect(screen.getByText("Downloading 42%")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /continue/i })).toBeDisabled();
    },
  );

  it("requires both a pasted outcome and text change, and retains the trial while editing", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await user.click(await screen.findByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    const trial = await screen.findByRole("textbox", { name: "Try a dictation" });
    const outcome = (kind: string) =>
      act(() => {
        for (const handler of eventListeners.get("paste-outcome") ?? [])
          handler({ payload: { outcome: kind, words: 3 } });
      });
    outcome("pasted");
    expect(screen.queryByText(/Worked/)).not.toBeInTheDocument();
    fireEvent.change(trial, { target: { value: "a real change" } });
    expect(screen.getByText("Worked · 3 words")).toBeInTheDocument();
    act(() => {
      for (const handler of eventListeners.get("recording-started") ?? [])
        handler({ payload: null });
    });
    await user.type(trial, " more");
    expect(screen.queryByText(/Worked/)).not.toBeInTheDocument();
    outcome("left_in_clipboard");
    await user.type(trial, " text");
    expect(screen.queryByText(/Worked/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Change shortcut" }));
    expect(screen.getByRole("button", { name: /save hotkey/i })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));
    expect(await screen.findByRole("textbox", { name: "Try a dictation" })).toHaveValue(
      "a real change more text",
    );
    expect(screen.getByRole("textbox", { name: "Try a dictation" })).toHaveFocus();
  });

  it("saves the Windows shortcut once while registration is pending", async () => {
    Object.assign(platformMock, { isMacOS: false, isWindows: true });
    const user = userEvent.setup();
    renderOnboarding();
    await user.click(await screen.findByRole("button", { name: /continue/i }));
    await user.click(screen.getByRole("button", { name: /continue/i }));
    let resolveSave: (() => void) | undefined;
    const pendingSave = new Promise<void>((resolve) => {
      resolveSave = resolve;
    });
    invokeMock.mockImplementation((command: string) =>
      command === "set_global_shortcut" ? pendingSave : Promise.resolve(null),
    );
    await user.click(screen.getByRole("button", { name: /continue/i }));
    expect(
      invokeMock.mock.calls.filter(([command]) => command === "set_global_shortcut"),
    ).toHaveLength(1);
    await act(async () => {
      resolveSave?.();
      await pendingSave;
    });
    await screen.findByRole("textbox", { name: "Try a dictation" });
    expect(
      invokeMock.mock.calls.filter(([command]) => command === "set_global_shortcut"),
    ).toHaveLength(1);
  });
});
