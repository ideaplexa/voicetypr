import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { toast } from "sonner";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LicenseStatus, ModelInfo } from "@/types";
import { OnboardingDesktop } from "./OnboardingDesktop";
import type { OnboardingDesktopProps } from "./onboardingTypes";

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

const permissionState = vi.hoisted(() => ({
  microphone: "granted" as string,
  accessibility: true,
  requestMicrophone: vi.fn(),
}));

vi.mock("@/hooks/useMicrophonePermission", () => ({
  useMicrophonePermission: () => ({
    hasPermission: permissionState.microphone === "granted",
    permissionStatus: permissionState.microphone,
    checkPermission: vi.fn().mockResolvedValue(permissionState.microphone === "granted"),
    requestPermission: permissionState.requestMicrophone,
  }),
}));

vi.mock("@/hooks/useAccessibilityPermission", () => ({
  useAccessibilityPermission: () => ({
    hasPermission: permissionState.accessibility,
    checkPermission: vi.fn().mockResolvedValue(permissionState.accessibility),
    requestPermission: vi.fn().mockResolvedValue(permissionState.accessibility),
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

const shellOpen = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: shellOpen }));

const platformMock = vi.hoisted(() => ({ isMacOS: true, isWindows: false, isLinux: false }));
vi.mock("@/lib/platform", () => platformMock);

const licensed: LicenseStatus = { status: "licensed", license_type: "pro" };

const renderOnboarding = (props: Partial<OnboardingDesktopProps> = {}) =>
  render(
    <OnboardingDesktop
      onCompletionStart={onCompletionStartMock}
      onCompletionError={onCompletionErrorMock}
      onComplete={onCompleteMock}
      licenseStatus={licensed}
      modelManagement={modelManagement as never}
      {...props}
    />,
  );

type User = ReturnType<typeof userEvent.setup>;

const openSource = async (user: User) => {
  await user.click(await screen.findByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: /where should your voice become text/i });
};

const BUILTIN_READY = { will_run: false, own_provider_ready: false, own_provider_enabled: false, builtin_enabled: true, builtin: "ready" };
let polishStatus: Record<string, unknown> = BUILTIN_READY;

const ENGINE_HEADING = /^choose (your model|your cloud provider|the computer)$/i;

// Where → language → engine.
const throughLanguage = async (user: User) => {
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: "What language do you speak?" });
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: ENGINE_HEADING });
};

const openEngine = async (user: User) => {
  await openSource(user);
  await throughLanguage(user);
};

const pickSource = async (user: User, name: RegExp) => {
  await user.click(screen.getByRole("button", { name }));
  await throughLanguage(user);
};

const toShortcut = async (user: User) => {
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: "Your shortcut" });
};

const reachSetup = async (user: User) => {
  await openEngine(user);
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: /now let it type anywhere/i });
};

// From Setup through the shortcut page and the fork, choosing "Go deeper".
const reachPolish = async (user: User) => {
  await reachSetup(user);
  await toShortcut(user);
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await user.click(await screen.findByRole("button", { name: "Go deeper" }));
  await screen.findByRole("heading", { name: "What will you mostly dictate?" });
};

// From the shortcut page through the fork, choosing "Done for now".
const skipUse = async (user: User) => {
  await screen.findByRole("heading", { name: "Your shortcut" });
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: "The essentials are set" });
  await user.click(screen.getByRole("button", { name: "Done for now" }));
};

const finishSetup = async (user: User) => {
  await reachSetup(user);
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await skipUse(user);
  await user.click(await screen.findByRole("button", { name: /skip for now/i }));
};

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  Object.assign(permissionState, { microphone: "granted", accessibility: true });
  permissionState.requestMicrophone.mockResolvedValue(true);
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
  delete (settingsState as Record<string, unknown>).builtin_polish_enabled;
  polishStatus = BUILTIN_READY;
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
      case "polish_status":
        return Promise.resolve(polishStatus);
      default:
        return Promise.resolve(null);
    }
  });
});

describe("OnboardingDesktop", () => {
  it("saves the hotkey and reaches the end without a trial step for a licensed device", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await finishSetup(user);

    await screen.findByRole("heading", { name: /welcome back/i });
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

    await finishSetup(user);

    await screen.findByRole("heading", { name: /welcome back/i });

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
        licenseStatus={licensed}
        modelManagement={modelManagement as never}
      />,
    );

    await finishSetup(user);

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

    await openSource(user);
    await pickSource(user, /another computer/i);
    await user.click(await screen.findByRole("button", { name: /use this server/i }));
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await screen.findByRole("heading", { name: /now let it type anywhere/i });
    await toShortcut(user);

    await user.click(await screen.findByRole("button", { name: /^Change$/ }));
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
    await finishSetup(user);

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
    await openEngine(user);
    expect(screen.getByRole("heading", { name: "Choose your model" })).toBeInTheDocument();
    expect(screen.getByText("In use")).toBeInTheDocument();
    expect(settingsState.current_model).toBe("base.en");
    expect(updateSettingsMock).not.toHaveBeenCalledWith(
      expect.objectContaining({ current_model: expect.any(String) }),
    );
  });

  it("shows local, cloud, and remote as explicit source choices", async () => {
    const user = userEvent.setup();
    renderOnboarding();

    await openSource(user);

    expect(
      screen.getByRole("heading", { name: /where should your voice become text/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /on this Mac/i })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: /^Cloud, with your key$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /another computer/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeEnabled();
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
    await openSource(user);
    await user.click(screen.getByRole("button", { name: /^Cloud, with your key$/i }));

    await act(async () => {
      resolveActiveRemote("remote-1");
      await activeRemote;
    });

    expect(screen.getByRole("button", { name: /^Cloud, with your key$/i })).toHaveAttribute(
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
    await openEngine(user);
    await user.click(await screen.findByRole("button", { name: "Use Base English" }));

    await act(async () => {
      resolveActiveRemote("remote-1");
      await activeRemote;
    });

    expect(screen.getByRole("heading", { name: "Choose your model" })).toBeInTheDocument();
    expect(updateSettingsMock).toHaveBeenCalledWith({
      current_model: "base.en",
      current_model_engine: "whisper",
    });
  });

  it("selects the recommended ready local model when the source step continues", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    renderOnboarding();

    await openEngine(user);
    await user.click(screen.getByRole("button", { name: /^continue$/i }));

    expect(updateSettingsMock).toHaveBeenCalledWith({ current_model: "base.en", current_model_engine: "whisper" });
    expect(await screen.findByRole("heading", { name: /now let it type anywhere/i })).toBeInTheDocument();
  });

  it("goes back to the source step from the first try while the model downloads, and returns", async () => {
    const user = userEvent.setup();
    modelManagement.models["base.en"].downloaded = false;
    renderOnboarding();

    await reachSetup(user);
    await waitFor(() => expect(modelManagement.downloadModel).toHaveBeenCalledWith("base.en"));
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await skipUse(user);
    expect(await screen.findByRole("heading", { name: "Almost ready" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /use something else/i }));

    expect(await screen.findByRole("heading", { name: /where should your voice become text/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /on this Mac/i })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /^Cloud, with your key$/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /another computer on your network/i })).toBeInTheDocument();
    expect(screen.getByText(/on a trusted network/i)).toBeInTheDocument();

    await throughLanguage(user);
    expect(screen.getByRole("heading", { name: "Choose your model" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(await screen.findByRole("heading", { name: "Almost ready" })).toBeInTheDocument();
    expect(modelManagement.downloadModel).toHaveBeenCalledTimes(1);
  });

  it("stops using the network computer when the user picks this Mac and goes back", async () => {
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
      command === "get_active_remote_server" ? Promise.resolve("remote-1") : base?.(command, ...rest),
    );
    const user = userEvent.setup();
    renderOnboarding();
    await openSource(user);
    await waitFor(() => expect(screen.getByRole("button", { name: /another computer/i })).toHaveAttribute("aria-pressed", "true"));
    await user.click(screen.getByRole("button", { name: /on this Mac/i }));
    await user.click(screen.getByRole("button", { name: /^back$/i }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_active_remote_server", { serverId: null }));
  });

  it("lists the models on the source step and downloads the one picked there", async () => {
    modelManagement.models["small.en"] = {
      ...modelManagement.models["base.en"],
      name: "small.en",
      display_name: "Small English",
      size: 466 * 1024 * 1024,
      downloaded: false,
      accuracy_score: 8,
      speed_score: 5,
    } as ModelInfo;
    modelManagement.modelOrder = ["base.en", "small.en"];
    const user = userEvent.setup();
    const view = renderOnboarding();
    await openEngine(user);
    expect(screen.getByText("In use")).toBeInTheDocument();
    expect(screen.getByText("Most accurate · English only · 466 MB")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /repair|delete/i })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Download Small English" }));
    await waitFor(() => expect(modelManagement.downloadModel).toHaveBeenCalledWith("small.en"));
    const rerender = () => view.rerender(<OnboardingDesktop onComplete={onCompleteMock} licenseStatus={licensed} modelManagement={modelManagement as never} />);
    modelManagement.downloadProgress = { "small.en": 30 };
    rerender();
    expect(screen.getByRole("progressbar", { name: "Downloading Small English" })).toBeInTheDocument();
    expect(screen.getByText("30%")).toBeInTheDocument();
    expect(settingsState.current_model).toBe("base.en");
    modelManagement.downloadProgress = {};
    modelManagement.models["small.en"] = { ...modelManagement.models["small.en"], downloaded: true };
    rerender();
    await waitFor(() => expect(settingsState.current_model).toBe("small.en"));
  });

  it("ranks models only among those that speak the chosen language and names the one it warns about", async () => {
    settingsState.speech_language = "fr";
    localStorage.setItem("onboarding-language-chosen", "true");
    modelManagement.models["small.en"] = {
      ...modelManagement.models["base.en"],
      name: "small.en",
      display_name: "Small English",
      size: 466 * 1024 * 1024,
      downloaded: false,
      accuracy_score: 9,
      speed_score: 9,
    } as ModelInfo;
    modelManagement.models.base = {
      ...modelManagement.models["base.en"],
      name: "base",
      display_name: "Base",
      size: 142 * 1024 * 1024,
      downloaded: false,
      accuracy_score: 6,
      speed_score: 4,
    } as ModelInfo;
    modelManagement.modelOrder = ["base.en", "small.en", "base"];
    const user = userEvent.setup();
    renderOnboarding();
    await openEngine(user);
    expect(screen.getByText("English only · 466 MB")).toBeInTheDocument();
    expect(screen.getByText("Most accurate · Fastest · 142 MB")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Download Small English" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Small English doesn't support your chosen language.");
  });

  it("lets a cloud choice stand when a model picked earlier finishes downloading", async () => {
    modelManagement.models["small.en"] = {
      ...modelManagement.models["base.en"],
      name: "small.en",
      display_name: "Small English",
      downloaded: false,
    } as ModelInfo;
    modelManagement.modelOrder = ["base.en", "small.en"];
    const user = userEvent.setup();
    const view = renderOnboarding();
    await openEngine(user);
    await user.click(screen.getByRole("button", { name: "Download Small English" }));
    await waitFor(() => expect(modelManagement.downloadModel).toHaveBeenCalledWith("small.en"));
    await user.click(screen.getByRole("button", { name: /^back$/i }));
    await user.click(await screen.findByRole("button", { name: /^back$/i }));
    await user.click(await screen.findByRole("button", { name: /^Cloud, with your key$/i }));
    modelManagement.models["small.en"] = { ...modelManagement.models["small.en"], downloaded: true };
    view.rerender(<OnboardingDesktop onComplete={onCompleteMock} licenseStatus={licensed} modelManagement={modelManagement as never} />);
    await act(async () => {});
    expect(updateSettingsMock).not.toHaveBeenCalledWith(expect.objectContaining({ current_model: "small.en" }));
  });

  it("lets remote-first users switch to local setup from the empty remote state", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    renderOnboarding();

    await openSource(user);
    await pickSource(user, /another computer/i);

    await user.click(await screen.findByRole("button", { name: /choose local instead/i }));

    expect(await screen.findByRole("heading", { name: "Choose your model" })).toBeInTheDocument();
    expect(screen.getByText("Base English")).toBeInTheDocument();
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

    await openSource(user);
    await pickSource(user, /another computer/i);
    expect(screen.getByText(/use only on a trusted network/i)).toBeInTheDocument();

    const useServerButton = await screen.findByRole("button", {
      name: /use this server/i,
    });
    await user.click(useServerButton);

    expect(invokeMock).toHaveBeenCalledWith("set_active_remote_server", {
      serverId: "remote-1",
    });
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(await screen.findByRole("heading", { name: /now let it type anywhere/i })).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("set_active_remote_server", { serverId: null });
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
    await openSource(user);
    await pickSource(user, /^Cloud, with your key$/i);
    expect(screen.getByText(/your audio goes to that provider/i)).toBeInTheDocument();
    for (const provider of ["Soniox", "Deepgram", "OpenAI", "Groq", "Cohere"]) {
      expect(screen.getByText(provider)).toBeInTheDocument();
    }
    await user.click(screen.getByRole("button", { name: "Add key for Soniox" }));
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
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeEnabled();
  });

  it("Windows GPU toggle ON→OFF persists 'cpu' (default state: acceleration undefined → switch ON)", async () => {
    platformMock.isMacOS = false;
    platformMock.isWindows = true;
    modelManagement.models["base.en"].gpu_controls_available = true;
    // settingsState.transcription_acceleration is undefined → switch resolves to ON
    const user = userEvent.setup();
    renderOnboarding();

    await openEngine(user);

    const gpuSwitch = screen.getByRole("switch", { name: /use gpu acceleration/i });
    expect(gpuSwitch).toBeInTheDocument();
    expect(gpuSwitch).toHaveAttribute("aria-checked", "true");

    await user.click(gpuSwitch);
    expect(updateSettingsMock).toHaveBeenCalledWith({ transcription_acceleration: "cpu" });
  });

  it("Windows GPU toggle OFF→ON persists 'auto' (prior state: acceleration 'cpu' → switch OFF)", async () => {
    platformMock.isMacOS = false;
    platformMock.isWindows = true;
    modelManagement.models["base.en"].gpu_controls_available = true;
    (settingsState as Record<string, unknown>).transcription_acceleration = "cpu";
    const user = userEvent.setup();
    renderOnboarding();

    await openEngine(user);

    const gpuSwitch = screen.getByRole("switch", { name: /use gpu acceleration/i });
    expect(gpuSwitch).toHaveAttribute("aria-checked", "false");

    await user.click(gpuSwitch);
    expect(updateSettingsMock).toHaveBeenCalledWith({ transcription_acceleration: "auto" });
  });

  it("hides GPU acceleration for backend-reported Windows ONNX Parakeet", async () => {
    platformMock.isMacOS = false;
    platformMock.isWindows = true;
    modelManagement.models["base.en"] = {
      ...modelManagement.models["base.en"],
      engine: "parakeet",
      runtime: "onnx",
      gpu_controls_available: false,
    };
    const user = userEvent.setup();
    renderOnboarding();
    await openEngine(user);
    expect(screen.queryByRole("switch", { name: /use gpu acceleration/i })).not.toBeInTheDocument();
  });

  it("does not show the GPU toggle on macOS", async () => {
    // platformMock defaults to isMacOS:true, isWindows:false (reset by beforeEach)
    const user = userEvent.setup();
    renderOnboarding();

    // Navigate through macOS flow to readiness (welcome→source→permissions→readiness)
    await openSource(user);

    expect(screen.queryByText(/use gpu acceleration/i)).not.toBeInTheDocument();
  });

  it("saves isolated_tap binding when bare modifier captured with Hold to talk OFF", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    await toShortcut(user);
    await user.click(screen.getByRole("button", { name: /^Change$/ }));
    await user.click(screen.getByTitle("Change hotkey"));

    // Wait until the keydown listener has captured the bare modifier.
    fireEvent.keyDown(window, { key: "Alt", code: "AltRight", altKey: true });
    fireEvent.keyUp(window, { key: "Alt", code: "AltRight" });
    await waitFor(() => expect(screen.getByTitle("Save hotkey")).not.toBeDisabled());
    await user.click(screen.getByTitle("Save hotkey"));
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("button", { name: /^Change$/ });
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
    expect(screen.getByText(/Press to talk, press again to paste/)).toBeInTheDocument();
  });

  it("saves modifier_hold binding when bare modifier captured with Hold to talk ON", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    await toShortcut(user);
    await user.click(screen.getByRole("button", { name: /^Change$/ }));
    await user.click(screen.getByTitle("Change hotkey"));

    fireEvent.keyDown(window, { key: "Alt", code: "AltRight", altKey: true });
    fireEvent.keyUp(window, { key: "Alt", code: "AltRight" });
    await waitFor(() => expect(screen.getByTitle("Save hotkey")).not.toBeDisabled());
    await user.click(screen.getByTitle("Save hotkey"));
    await user.click(screen.getByRole("switch", { name: /hold to talk/i }));
    await user.click(screen.getByRole("button", { name: /save hotkey/i }));

    await screen.findByRole("button", { name: /^Change$/ });
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
    expect(screen.getByText(/Hold to talk, let go to paste/)).toBeInTheDocument();
  });

  it("starts the sharing choice unticked and saves it on completion", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await finishSetup(user);
    await screen.findByRole("heading", { name: /welcome back/i });

    expect(screen.getByRole("checkbox", { name: /share anonymous crash reports and usage numbers/i })).not.toBeChecked();

    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: false });
      expect(invokeMock).toHaveBeenCalledWith("record_onboarding_completed");
    });
  });

  it("keeps a sharing choice the user already made when onboarding runs again", async () => {
    const user = userEvent.setup();
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
      command === "get_telemetry_status"
        ? Promise.resolve({ enabled: true, available: true, consent_required: false })
        : base?.(command, ...rest),
    );
    renderOnboarding();
    await finishSetup(user);
    await screen.findByRole("heading", { name: /welcome back/i });
    expect(screen.queryByRole("checkbox", { name: /share anonymous crash reports/i })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Start using Voicetypr" }));
    await waitFor(() => expect(settingsState.onboarding_completed).toBe(true));
    expect(invokeMock).not.toHaveBeenCalledWith("set_telemetry_consent", expect.anything());
  });

  it("saves the opt-in during onboarding", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await finishSetup(user);
    await screen.findByRole("heading", { name: /welcome back/i });

    await user.click(screen.getByRole("checkbox", { name: /share anonymous crash reports and usage numbers/i }));
    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: true });
    });
  });

  it("keeps onboarding active until the sharing choice is saved", async () => {
    const user = userEvent.setup();
    let resolveAnalytics: (() => void) | undefined;
    const analyticsPersisted = new Promise<void>((resolve) => {
      resolveAnalytics = resolve;
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "set_telemetry_consent") return analyticsPersisted;
      if (command === "discover_remote_servers" || command === "list_remote_servers" || command === "get_audio_devices") {
        return Promise.resolve([]);
      }
      return Promise.resolve(null);
    });
    renderOnboarding();

    await finishSetup(user);
    await user.click(await screen.findByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("set_telemetry_consent", { enabled: false });
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

  it("keeps onboarding recoverable when saving the sharing choice fails", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "set_telemetry_consent") {
        return Promise.reject(new Error("analytics consent store unavailable"));
      }
      if (command === "discover_remote_servers" || command === "list_remote_servers" || command === "get_audio_devices") {
        return Promise.resolve([]);
      }
      return Promise.resolve(null);
    });
    renderOnboarding();

    await finishSetup(user);
    await user.click(await screen.findByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => {
      expect(onCompletionErrorMock).toHaveBeenCalledTimes(1);
    });
    expect(invokeMock).not.toHaveBeenCalledWith("record_onboarding_completed");
    expect(onCompleteMock).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: /welcome back/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /start using voicetypr/i })).toBeEnabled();

    invokeMock.mockResolvedValue(null);
    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => {
      expect(onCompleteMock).toHaveBeenCalledTimes(1);
    });
    expect(onCompletionStartMock).toHaveBeenCalledTimes(2);
  });

  it("keeps the sharing choice when the completion save fails and succeeds on retry", async () => {
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

    await finishSetup(user);
    await user.click(await screen.findByRole("checkbox", { name: /share anonymous crash reports and usage numbers/i }));
    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => expect(onCompletionErrorMock).toHaveBeenCalledTimes(1));
    expect(settingsState.onboarding_completed).toBe(false);
    expect(screen.getByRole("checkbox", { name: /share anonymous crash reports and usage numbers/i })).toBeChecked();

    await user.click(screen.getByRole("button", { name: /start using voicetypr/i }));

    await waitFor(() => {
      expect(settingsState.onboarding_completed).toBe(true);
      expect(onCompleteMock).toHaveBeenCalledTimes(1);
    });
    expect(completionAttempts).toBe(2);
  });

  it.each(["macos", "windows"] as const)(
    "walks the promise, source, setup, Polish, first try and done on %s",
    async (platform) => {
      Object.assign(platformMock, {
        isMacOS: platform === "macos",
        isWindows: platform === "windows",
      });
      // A user who turned built-in Polish off and has no AI of their own.
      Object.assign(settingsState, { builtin_polish_enabled: false });
      const user = userEvent.setup();
      renderOnboarding();
      expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("Say it.It lands in any app.");
      await reachSetup(user);
      if (platform === "macos") {
        expect(screen.getByRole("heading", { name: "Accessibility" })).toBeInTheDocument();
      } else {
        expect(screen.queryByText("Accessibility")).not.toBeInTheDocument();
        expect(screen.getByRole("meter", { name: "Microphone input level" })).toBeInTheDocument();
      }
      await toShortcut(user);
      expect(screen.getByRole("button", { name: "Press to start and stop" })).toHaveAttribute("aria-pressed", "true");
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await user.click(await screen.findByRole("button", { name: "Go deeper" }));
      expect(await screen.findByRole("heading", { name: "What will you mostly dictate?" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^skip$/i }));
      expect(await screen.findByRole("heading", { name: "Say something" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /skip for now/i }));
      await screen.findByRole("heading", { name: /welcome back/i });
      await user.click(screen.getByRole("button", { name: "Start using Voicetypr" }));
      await waitFor(() => expect(settingsState.onboarding_completed).toBe(true));
    },
  );

  it("turns on built-in Polish for a new user and says the text goes to Voicetypr's server", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await waitFor(() => expect(updateSettingsMock).toHaveBeenCalledWith({ builtin_polish_enabled: true }));
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("Say it messy.It lands clean.");
    await reachSetup(user);
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await skipUse(user);
    expect(await screen.findByRole("heading", { name: "Say something messy" })).toBeInTheDocument();
    expect(screen.getByText(/Polish sends your words to Voicetypr’s server/)).toBeInTheDocument();
  });

  it("leaves built-in Polish off when onboarding reopens for an existing user", async () => {
    Object.assign(settingsState, { onboarding_completed: true });
    renderOnboarding();
    await screen.findByRole("heading", { level: 1 });
    expect(updateSettingsMock).not.toHaveBeenCalledWith({ builtin_polish_enabled: true });
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Say it.It lands in any app.");
  });

  it("keeps built-in Polish off for an existing user after Setup saves onboarding as unfinished", async () => {
    const user = userEvent.setup();
    Object.assign(settingsState, { onboarding_completed: true });
    renderOnboarding();
    await reachPolish(user);
    expect(settingsState.onboarding_completed).toBe(false);
    expect(updateSettingsMock).not.toHaveBeenCalledWith({ builtin_polish_enabled: true });
    expect(screen.getByRole("switch", { name: "Clean up what I say" })).not.toBeChecked();
  });

  it("says so when switching built-in Polish off could not be saved, and keeps it showing on", async () => {
    const user = userEvent.setup();
    const toastError = vi.spyOn(toast, "error");
    renderOnboarding();
    await reachPolish(user);
    updateSettingsMock.mockImplementationOnce(() => Promise.reject(new Error("disk")));
    await user.click(screen.getByRole("switch", { name: "Clean up what I say" }));
    await waitFor(() => expect(toastError).toHaveBeenCalledWith("Couldn’t save the Polish switch. Try again."));
    expect(screen.getByRole("switch", { name: "Clean up what I say" })).toBeChecked();
  });

  it("keeps a user's earlier choice to leave built-in Polish off", async () => {
    Object.assign(settingsState, { builtin_polish_enabled: false });
    renderOnboarding();
    await screen.findByRole("heading", { level: 1 });
    expect(updateSettingsMock).not.toHaveBeenCalledWith({ builtin_polish_enabled: true });
    expect(screen.queryByText(/Voicetypr’s server/)).not.toBeInTheDocument();
  });

  it.each([
    ["macos", "Alt+Space", "⌥"],
    ["windows", "Control+Space", "Ctrl"],
  ] as const)("uses the platform default shortcut in the copy on %s", async (platform, hotkey, cap) => {
    Object.assign(platformMock, { isMacOS: platform === "macos", isWindows: platform === "windows" });
    settingsState.hotkey = "";
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    await toShortcut(user);
    expect((await screen.findAllByLabelText(`${cap} + Space`)).length).toBeGreaterThan(0);
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("set_global_shortcut", { shortcut: hotkey }));
    await user.click(await screen.findByRole("button", { name: "Done for now" }));
    expect(await screen.findByText(/Press again when you’re done/)).toBeInTheDocument();
  });

  it.each(["macos", "windows"] as const)(
    "downloads the engine on the model step, keeps its progress in view, and blocks the first try until ready on %s",
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
      const rerender = () => view.rerender(
        <OnboardingDesktop
          onComplete={onCompleteMock}
          licenseStatus={licensed}
          modelManagement={modelManagement as never}
        />,
      );
      const begin = await screen.findByRole("button", { name: /^continue$/i });
      expect(modelManagement.downloadModel).not.toHaveBeenCalled();
      await user.click(begin);
      await screen.findByRole("heading", { name: /where should your voice become text/i });
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("heading", { name: "What language do you speak?" });
      expect(modelManagement.downloadModel).not.toHaveBeenCalled();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByText(/670 MB$/)).toBeInTheDocument();
      await waitFor(() => expect(modelManagement.downloadModel).toHaveBeenCalledWith("base.en"));
      modelManagement.downloadProgress = { "base.en": 42 };
      rerender();
      expect(screen.getByRole("progressbar", { name: /^Downloading / })).toBeInTheDocument();
      expect(screen.getByText("42%")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: /now let it type anywhere/i })).toBeInTheDocument();
      expect(screen.getByText("Dictation engine · 42% of 670 MB")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: "Your shortcut" })).toBeInTheDocument();
      expect(screen.getByText("Dictation engine · 42% of 670 MB")).toBeInTheDocument();
      await skipUse(user);
      expect(await screen.findByRole("heading", { name: "Almost ready" })).toBeInTheDocument();
      expect(modelManagement.downloadModel).toHaveBeenCalledTimes(1);
      expect(screen.getByText("Getting the dictation engine ready · 42% of 670 MB. This happens once.")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /retry/i })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /click here to talk/i })).not.toBeInTheDocument();

      modelManagement.downloadProgress = {};
      Object.assign(modelManagement, { downloadErrors: { "base.en": "error sending request: connection reset" } });
      view.rerender(
        <OnboardingDesktop
          onComplete={onCompleteMock}
          licenseStatus={licensed}
          modelManagement={modelManagement as never}
        />,
      );
      expect(screen.getByRole("alert")).toHaveTextContent(
        "The download stopped. The connection dropped. Check your internet and try again.",
      );
      expect(screen.queryByText(/connection reset/)).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /retry/i }));
      expect(modelManagement.downloadModel).toHaveBeenCalledTimes(2);

      Object.assign(modelManagement, { downloadErrors: {} });
      view.rerender(
        <OnboardingDesktop
          onComplete={onCompleteMock}
          licenseStatus={licensed}
          modelManagement={modelManagement as never}
        />,
      );
      expect(screen.getByText("The dictation engine (670 MB) isn’t downloaded yet.")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Download" })).toBeInTheDocument();
      delete (modelManagement as { downloadErrors?: unknown }).downloadErrors;
      modelManagement.downloadProgress = { "base.en": 42 };
      rerender();
      await user.click(screen.getByRole("button", { name: /skip for now/i }));
      expect(await screen.findByRole("heading", { name: /welcome back/i })).toBeInTheDocument();
      expect(screen.getByText("Dictation engine · 42% of 670 MB")).toBeInTheDocument();
    },
  );

  it("keeps the step count when a trial starts mid-flow", async () => {
    const view = renderOnboarding({ licenseStatus: { status: "none" } });
    expect(await screen.findByRole("progressbar", { name: "Setup phase" })).toHaveAttribute("aria-valuemax", "11");
    view.rerender(
      <OnboardingDesktop
        onComplete={onCompleteMock}
        licenseStatus={{ status: "trial", trial_days_left: 7 }}
        modelManagement={modelManagement as never}
      />,
    );
    expect(screen.getByRole("progressbar", { name: "Setup phase" })).toHaveAttribute("aria-valuemax", "11");
  });

  const reachTry = async (user: User) => {
    await reachSetup(user);
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await skipUse(user);
    await screen.findByRole("button", { name: /click here to talk/i });
  };
  const emit = (event: string, payload: unknown) =>
    act(() => {
      for (const handler of eventListeners.get(event) ?? []) handler({ payload });
    });

  it("shows the engine's own text when Polish is off", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await reachTry(user);
    emit("transcription-added", { text: "um so Friday at three", writing: { ai_applied: false } });
    expect(await screen.findByText("um so Friday at three")).toBeInTheDocument();
    expect(screen.getByText("Voicetypr typed")).toBeInTheDocument();
    expect(screen.queryByText("You said")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Worked · 5 words · exactly what the engine heard");
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(await screen.findByRole("heading", { name: /welcome back/i })).toBeInTheDocument();
  });

  it("falls back to the plain first try when built-in Polish didn't run", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await reachTry(user);
    expect(await screen.findByRole("heading", { name: "Say something messy" })).toBeInTheDocument();
    expect(screen.getByText("You said")).toBeInTheDocument();
    emit("transcription-added", { text: "um so Friday at three", writing: { ai_applied: false } });
    expect(await screen.findByText("um so Friday at three")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Say something" })).toBeInTheDocument();
    expect(screen.getByText("Voicetypr typed")).toBeInTheDocument();
    expect(screen.queryByText("You said")).not.toBeInTheDocument();
  });

  it("puts what was said beside the cleaned text when Polish changed it", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await reachTry(user);
    expect(await screen.findByRole("heading", { name: "Say something messy" })).toBeInTheDocument();
    emit("transcription-added", {
      text: "Let’s move the sync to Friday at 3.",
      writing: { ai_applied: true, original_text: "um so let's move the sync to thursday no friday at three" },
    });
    expect(await screen.findByText("um so let's move the sync to thursday no friday at three")).toBeInTheDocument();
    expect(screen.getByText("Let’s move the sync to Friday at 3.")).toBeInTheDocument();
    expect(screen.getByText("You said")).toBeInTheDocument();
    expect(screen.getByText("Voicetypr cleaned it")).toBeInTheDocument();
  });

  it("offers Recheck, never Trial ended, when the license check failed", async () => {
    const recheck = vi.fn(async () => undefined);
    const user = userEvent.setup();
    renderOnboarding({ licenseCheckFailed: true, onRecheckLicense: recheck });
    await reachSetup(user);
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await skipUse(user);
    expect(await screen.findByRole("heading", { name: "We couldn’t check your license" })).toBeInTheDocument();
    expect(screen.queryByText("Trial ended")).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([command]) => command === "start_recording")).toBe(false);
    await user.click(screen.getByRole("button", { name: "Recheck" }));
    expect(recheck).toHaveBeenCalledTimes(1);
  });

  it("clears the network route when this Mac is chosen, even before the saved route has loaded", async () => {
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
      command === "get_active_remote_server" ? new Promise(() => undefined) : base?.(command, ...rest),
    );
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    expect(invokeMock).toHaveBeenCalledWith("set_active_remote_server", { serverId: null });
  });

  it("asks for the microphone on the first try, then starts dictation", async () => {
    permissionState.microphone = "not_determined";
    const user = userEvent.setup();
    renderOnboarding();
    await reachTry(user);
    await user.click(screen.getByRole("button", { name: /click here to talk/i }));
    expect(permissionState.requestMicrophone).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("start_recording", { source: "pointer" });
  });

  it("explains a denied microphone and opens its settings", async () => {
    permissionState.microphone = "denied";
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await skipUse(user);
    expect(await screen.findByText(/The microphone is off for Voicetypr/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /click here to talk/i })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Open System Settings" }));
    expect(invokeMock).toHaveBeenCalledWith("open_microphone_settings");
  });

  it("offers an explicit Stop while dictating, without starting another recording", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await reachTry(user);
    emit("recording-state-changed", { state: "recording", error: null, generation: 1 });
    await user.click(await screen.findByRole("button", { name: "Stop" }));
    expect(invokeMock).toHaveBeenCalledWith("stop_recording");
    expect(invokeMock.mock.calls.some(([command]) => command === "start_recording")).toBe(false);
  });

  it("lets Accessibility be skipped and says how to paste", async () => {
    permissionState.accessibility = false;
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: /skip, i’ll paste myself/i }));
    await screen.findByRole("heading", { name: "Your shortcut" });
    expect(screen.getByText(/Press to talk, press again to copy/)).toBeInTheDocument();
    await skipUse(user);
    await user.click(await screen.findByRole("button", { name: /skip for now/i }));
    await screen.findByRole("heading", { name: /welcome back/i });
    expect(screen.getByText(/press ⌘V to paste them/)).toBeInTheDocument();
  });

  it("saves the Windows shortcut once while registration is pending", async () => {
    Object.assign(platformMock, { isMacOS: false, isWindows: true });
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    await toShortcut(user);
    let resolveSave: (() => void) | undefined;
    const pendingSave = new Promise<void>((resolve) => {
      resolveSave = resolve;
    });
    invokeMock.mockImplementation((command: string) =>
      command === "set_global_shortcut" ? pendingSave : Promise.resolve(null),
    );
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    expect(
      invokeMock.mock.calls.filter(([command]) => command === "set_global_shortcut"),
    ).toHaveLength(1);
    await act(async () => {
      resolveSave?.();
      await pendingSave;
    });
    await user.click(await screen.findByRole("button", { name: "Done for now" }));
    await screen.findByRole("heading", { name: "Say something" });
    expect(
      invokeMock.mock.calls.filter(([command]) => command === "set_global_shortcut"),
    ).toHaveLength(1);
  });

  describe("trial step", () => {
    const fillContact = async (user: User) => {
      await user.type(await screen.findByLabelText("Name"), "Priya Raman");
      await user.type(screen.getByLabelText("Email"), "priya@example.com");
    };
    // The trial and Upgrade are one last screen after the essentials and the fork.
    const reachTrial = async (user: User) => {
      await reachSetup(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await skipUse(user);
      await screen.findByRole("heading", { name: /^(Start with 7 days free|Trial ended)$/ });
    };

    it("asks for the trial after the essentials, and the first try works as soon as it starts", async () => {
      const start = vi.fn().mockResolvedValue(null);
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" }, onStartTrial: start });
      await openSource(user);
      expect(screen.queryByRole("button", { name: "Start 7-day trial" })).not.toBeInTheDocument();
      await throughLanguage(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("heading", { name: /now let it type anywhere/i });
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await skipUse(user);
      await screen.findByRole("heading", { name: "Start with 7 days free" });
      expect(screen.getByRole("button", { name: "Upgrade now" })).toBeInTheDocument();
      expect(screen.queryByRole("checkbox", { name: /share anonymous crash reports/i })).not.toBeInTheDocument();
      expect(invokeMock.mock.calls.some(([command]) => command === "start_recording")).toBe(false);
      await fillContact(user);
      await user.click(screen.getByRole("button", { name: "Start 7-day trial" }));
      expect(start).toHaveBeenCalledWith("Priya Raman", "priya@example.com");
      await screen.findByRole("heading", { name: /^Say something/ });
      await user.click(screen.getByRole("button", { name: /click here to talk/i }));
      expect(invokeMock).toHaveBeenCalledWith("start_recording", { source: "pointer" });
      await user.click(screen.getByRole("button", { name: /skip for now/i }));
      expect(await screen.findByRole("heading", { name: "You’re in, Priya" })).toBeInTheDocument();
    });

    it("never dictates before a trial or key, and offers the way back to the trial", async () => {
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" }, onStartTrial: vi.fn() });
      await reachTrial(user);
      await user.click(screen.getByRole("button", { name: "Decide later" }));
      expect(await screen.findByRole("heading", { name: "Dictation starts with your trial" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /click here to talk/i })).not.toBeInTheDocument();
      expect(invokeMock.mock.calls.some(([command]) => command === "start_recording")).toBe(false);
      await user.click(screen.getByRole("button", { name: "Start 7-day trial" }));
      expect(await screen.findByRole("heading", { name: "Start with 7 days free" })).toBeInTheDocument();
    });

    it("keeps the user on the trial step and never says the trial ended when the start fails", async () => {
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: null, onStartTrial: vi.fn().mockResolvedValue("trial_unreachable") });
      await reachTrial(user);
      await fillContact(user);
      await user.click(screen.getByRole("button", { name: "Start 7-day trial" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("Couldn’t reach Voicetypr, so your trial hasn’t started");
      expect(screen.queryByText(/expired|trial ended/i)).not.toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "Start with 7 days free" })).toBeInTheDocument();
      expect(screen.getByLabelText("Email")).toHaveValue("priya@example.com");
      await user.click(screen.getByRole("button", { name: "Decide later" }));
      expect(await screen.findByRole("heading", { name: "We couldn’t confirm your trial" })).toBeInTheDocument();
      expect(screen.queryByText(/expired|trial ended/i)).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /click here to talk/i })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Try again" }));
      expect(await screen.findByRole("heading", { name: "Start with 7 days free" })).toBeInTheDocument();
    });

    it("skips the trial step when this device already has a license", async () => {
      const user = userEvent.setup();
      renderOnboarding({ onStartTrial: vi.fn() });
      await reachSetup(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await skipUse(user);
      expect(await screen.findByRole("heading", { name: /^Say something/ })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Start 7-day trial" })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /skip for now/i }));
      expect(await screen.findByRole("heading", { name: "Welcome back. Your license is active on this Mac." })).toBeInTheDocument();
    });

    it.each([
      ["a new device", { status: "none" }],
      ["a device whose trial ended", { status: "expired" }],
    ] as const)("opens pricing and the key window with the cursor in the field on Upgrade on %s", async (_name, licenseStatus) => {
      const openPricing = vi.fn().mockResolvedValue(undefined);
      const recheck = vi.fn().mockResolvedValue(undefined);
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus, onOpenPricing: openPricing, onRecheckLicense: recheck });
      await reachTrial(user);
      await user.click(screen.getByRole("button", { name: "Upgrade now" }));
      expect(openPricing).toHaveBeenCalledTimes(1);
      await waitFor(() => expect(screen.getByRole("textbox", { name: "License key" })).toHaveFocus());
      await user.click(screen.getByRole("button", { name: "Recheck" }));
      expect(recheck).toHaveBeenCalledTimes(1);
    });

    it("shows a key error in plain words, then goes to the first try once a key activates", async () => {
      const activate = vi.fn()
        .mockResolvedValueOnce("That key isn’t right. Check it against your receipt email and paste it again.")
        .mockResolvedValueOnce(null);
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" }, onActivateLicense: activate });
      await reachTrial(user);
      await user.click(screen.getByRole("button", { name: "I have a key" }));
      await user.type(screen.getByRole("textbox", { name: "License key" }), "VT-BAD");
      await user.click(screen.getByRole("button", { name: "Activate" }));
      expect(await screen.findByText(/That key isn’t right/)).toBeInTheDocument();
      await user.clear(screen.getByRole("textbox", { name: "License key" }));
      await user.type(screen.getByRole("textbox", { name: "License key" }), "VT-GOOD");
      await user.click(screen.getByRole("button", { name: "Activate" }));
      expect(activate).toHaveBeenLastCalledWith("VT-GOOD");
      expect(await screen.findByRole("heading", { name: /^Say something/ })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /skip for now/i }));
      expect(await screen.findByRole("heading", { name: "License active. Voicetypr is yours." })).toBeInTheDocument();
    });

    it.each([
      ["a new device", { status: "none" }],
      ["a device whose trial ended", { status: "expired" }],
    ] as const)("signs the trial step from Moinul with the support address on %s", async (_name, licenseStatus) => {
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus, onStartTrial: vi.fn() });
      await reachTrial(user);
      expect(screen.getByText(/Made by Moinul\./)).toBeInTheDocument();
      expect(screen.queryByText(/within|Made by Moin\b/)).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "I read every email." }));
      expect(shellOpen).toHaveBeenCalledWith("mailto:support@voicetypr.com");
    });

    it("offers only Upgrade and a key when this device's trial has ended", async () => {
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "expired" }, onStartTrial: vi.fn() });
      await reachTrial(user);
      expect(screen.getByRole("heading", { name: "Trial ended" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Start 7-day trial" })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Upgrade now" })).toBeInTheDocument();
    });
  });

  describe("what you mostly dictate", () => {
    const reachUse = async (user: User) => {
      await reachSetup(user);
      await toShortcut(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await user.click(await screen.findByRole("button", { name: "Go deeper" }));
      await screen.findByRole("heading", { name: "What will you mostly dictate?" });
    };

    it("applies the pick at once when built-in Polish is on", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: "Messages and email" }));
      expect(screen.getByRole("status")).toHaveTextContent(/^Polish will use the Message style/);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("button", { name: /click here to talk/i });
      expect(invokeMock).toHaveBeenCalledWith("update_enhancement_options", { options: { preset: "Message" } });
      // Kept too, so a provider of the user's own starts in the same style.
      expect(updateSettingsMock).toHaveBeenCalledWith({ starting_polish_style: "Message" });
    });

    it("shows built-in Polish on by default and turns it off for the next dictation, as Settings does", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      const toggle = screen.getByRole("switch", { name: "Clean up what I say" });
      await waitFor(() => expect(toggle).toBeChecked());
      expect(screen.getByText(/Your words go to Voicetypr’s server to be cleaned, and nothing is kept/)).toBeInTheDocument();
      expect(screen.getByText("Rather use your own AI provider? Add its key in Settings → Polish.")).toBeInTheDocument();
      expect(screen.queryByText(/On: used because/)).not.toBeInTheDocument();
      await user.click(toggle);
      // The same saved setting Settings → Polish writes, which the next dictation reads.
      expect(updateSettingsMock).toHaveBeenCalledWith({ builtin_polish_enabled: false });
    });

    it.each([
      ["no_access", "Works during your trial or with a license."],
      ["not_entitled", "Not available on this device: built-in Polish needs a trial started with your email, or a license."],
      ["unreachable", "Voicetypr’s server can’t take Polish right now (offline or busy), so nothing is sent and you get exactly what you said."],
    ])("never shows built-in Polish as on when the server says %s", async (builtin, line) => {
      polishStatus = { ...BUILTIN_READY, builtin };
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      expect(await screen.findByText(line)).toBeInTheDocument();
      const toggle = screen.getByRole("switch", { name: "Clean up what I say" });
      expect(toggle).not.toBeChecked();
      expect(toggle).toHaveAttribute("aria-disabled", "true");
      await user.click(screen.getByRole("button", { name: "Code and tech notes" }));
      expect(screen.getByText("When Polish is on, it starts in the Code style.")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("button", { name: /click here to talk/i });
      expect(updateSettingsMock).toHaveBeenCalledWith({ starting_polish_style: "Code" });
      // Saved now, so the first take after the server answers uses the pick.
      expect(invokeMock).toHaveBeenCalledWith("update_enhancement_options", { options: { preset: "Code" } });
    });

    it("waits to apply the pick when the user's own provider is set up but off", async () => {
      polishStatus = { will_run: false, own_provider_ready: true, own_provider_enabled: false, builtin_enabled: false, builtin: "own_provider" };
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: "Code and tech notes" }));
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("button", { name: /click here to talk/i });
      expect(updateSettingsMock).toHaveBeenCalledWith({ starting_polish_style: "Code" });
      expect(invokeMock).not.toHaveBeenCalledWith("update_enhancement_options", expect.anything());
    });

    it("saves the pick as the style Polish will start in, without claiming Polish is on", async () => {
      // Built-in Polish turned off and no AI of the user's own.
      Object.assign(settingsState, { builtin_polish_enabled: false });
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      expect(screen.getByRole("switch", { name: "Clean up what I say" })).not.toBeChecked();
      expect(screen.getByText("Off: built-in Polish won’t send your text.")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Messages and email" }));
      expect(screen.getByRole("status")).toHaveTextContent(
        "When Polish is on, it starts in the Message style, tidy sentences in your own wording.",
      );
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("button", { name: /click here to talk/i });
      expect(updateSettingsMock).toHaveBeenCalledWith({ starting_polish_style: "Message" });
      // Built-in Polish could run, so turning it back on later cleans in the picked style.
      expect(invokeMock).toHaveBeenCalledWith("update_enhancement_options", { options: { preset: "Message" } });
    });

    it("offers the switch before any trial, and claims Polish on the first try only once its allowance is open", async () => {
      polishStatus = { ...BUILTIN_READY, builtin: "first_try", will_run: false };
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      const toggle = screen.getByRole("switch", { name: "Clean up what I say" });
      await waitFor(() => expect(toggle).toBeChecked());
      expect(toggle).toBeEnabled();
      expect(screen.queryByText("Works during your trial or with a license.")).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: "Say something" })).toBeInTheDocument();
      expect(screen.queryByText(/Polish sends your words/)).not.toBeInTheDocument();
    });

    it("says the first try is polished on Voicetypr's server while the first-try allowance is open", async () => {
      polishStatus = { ...BUILTIN_READY, builtin: "first_try", will_run: true };
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: "Say something messy" })).toBeInTheDocument();
      expect(screen.getByText(/Polish sends your words to Voicetypr’s server/)).toBeInTheDocument();
    });

    it("sends nothing on the first try once Polish is switched off in the step before", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      expect(screen.getByText("You say")).toBeInTheDocument();
      await user.click(screen.getByRole("switch", { name: "Clean up what I say" }));
      expect(updateSettingsMock).toHaveBeenCalledWith({ builtin_polish_enabled: false });
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: "Say something" })).toBeInTheDocument();
      expect(screen.queryByText(/Polish sends your words/)).not.toBeInTheDocument();
      expect(screen.queryByText("You said")).not.toBeInTheDocument();
    });

    it("says on the first try where Polish can be turned off", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: "Say something messy" })).toBeInTheDocument();
      expect(screen.getByText(/Polish sends your words to Voicetypr’s server to clean them up, and nothing is kept\. You can turn this off in Settings → Polish\./)).toBeInTheDocument();
    });

    it("continues without a pick, keeping Polish on and today's default style", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      const toggle = screen.getByRole("switch", { name: "Clean up what I say" });
      await waitFor(() => expect(toggle).toBeChecked());
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("button", { name: /click here to talk/i });
      expect(updateSettingsMock).toHaveBeenCalledWith({ starting_polish_style: null });
      expect(updateSettingsMock).not.toHaveBeenCalledWith({ builtin_polish_enabled: false });
      expect(invokeMock).not.toHaveBeenCalledWith("update_enhancement_options", expect.anything());
    });

    it("leaves today's default when skipped", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: /^skip$/i }));
      await screen.findByRole("button", { name: /click here to talk/i });
      expect(updateSettingsMock).toHaveBeenCalledWith({ starting_polish_style: null });
    });

    it("sets the Polish style right away when Polish is already on", async () => {
      polishStatus = { will_run: true, own_provider_ready: true, own_provider_enabled: true, builtin_enabled: false, builtin: "own_provider" };
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: "Code and tech notes" }));
      expect(screen.getByRole("status")).toHaveTextContent("Polish will use the Code style.");
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await waitFor(() =>
        expect(invokeMock).toHaveBeenCalledWith("update_enhancement_options", { options: { preset: "Code" } }),
      );
    });
  });
});
