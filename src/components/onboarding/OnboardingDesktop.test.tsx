import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
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
const BUILTIN_READY = { will_run: true, own_provider_ready: false, own_provider_enabled: false, builtin_enabled: true, builtin: "ready" };
let polishStatus: Record<string, unknown> = BUILTIN_READY;

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
  await user.click(await screen.findByRole("button", { name: /^get started$/i }));
  await screen.findByRole("heading", { name: /where should your voice become text/i });
};

const ENGINE_HEADING = /^(choose a model for this (mac|pc)|choose a cloud provider|use another computer on your network)$/i;

// Where → language → engine.
const throughLanguage = async (user: User) => {
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: "Which language do you speak?" });
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: ENGINE_HEADING });
};

const openEngine = async (user: User) => {
  await openSource(user);
  await throughLanguage(user);
};

const pickSource = async (user: User, name: RegExp) => {
  await user.click(screen.getByRole("radio", { name }));
  await throughLanguage(user);
};

const toShortcut = async (user: User) => {
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: "Your shortcut" });
};

const reachSetup = async (user: User) => {
  await openEngine(user);
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: /let voicetypr hear you|can voicetypr hear you/i });
};

// From the shortcut page to the Polish switch.
const toPolish = async (user: User) => {
  await screen.findByRole("heading", { name: "Your shortcut" });
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await screen.findByRole("heading", { name: "Let AI tidy your words" });
};

// From the shortcut page through the Polish switch, as it stands, to the first try.
const skipUse = async (user: User) => {
  await toPolish(user);
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
};

const reachPolish = async (user: User) => {
  await reachSetup(user);
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await toPolish(user);
};

// From the first try, without dictating, to the fork's "Done for now".
const pastTry = async (user: User) => {
  await user.click(await screen.findByRole("button", { name: /skip for now/i }));
  await screen.findByRole("heading", { name: "That’s the essentials" });
  await user.click(screen.getByRole("button", { name: /^done for now/i }));
};

const finishSetup = async (user: User) => {
  await reachSetup(user);
  await user.click(screen.getByRole("button", { name: /^continue$/i }));
  await skipUse(user);
  await pastTry(user);
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
    // A fresh install: Rust turns built-in Polish on at startup.
    builtin_polish_enabled: true,
  });
  polishStatus = BUILTIN_READY;
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
    expect(onCompleteMock).toHaveBeenCalledWith(undefined);
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
    await user.click(await screen.findByRole("button", { name: /^use$/i }));
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await screen.findByRole("heading", { name: /let voicetypr hear you/i });
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
    expect(screen.getByRole("heading", { name: "Choose a model for this Mac" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Base English" })).toHaveAttribute("aria-checked", "true");
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
    expect(screen.getByRole("radio", { name: /^on this Mac/i })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: /^Cloud, with your key/i })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("radio", { name: /^another computer on your network/i })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByRole("progressbar", { name: "Setup progress" })).toHaveAttribute("aria-valuenow", "1");
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
    await user.click(screen.getByRole("radio", { name: /^Cloud, with your key/i }));

    await act(async () => {
      resolveActiveRemote("remote-1");
      await activeRemote;
    });

    expect(screen.getByRole("radio", { name: /^Cloud, with your key/i })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: /^another computer/i })).toHaveAttribute("aria-checked", "false");
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
    await user.click(await screen.findByRole("radio", { name: "Base English" }));

    await act(async () => {
      resolveActiveRemote("remote-1");
      await activeRemote;
    });

    expect(screen.getByRole("heading", { name: "Choose a model for this Mac" })).toBeInTheDocument();
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
    await user.click(screen.getByRole("button", { name: /^continue/i }));

    expect(updateSettingsMock).toHaveBeenCalledWith({ current_model: "base.en", current_model_engine: "whisper" });
    expect(await screen.findByRole("heading", { name: /let voicetypr hear you/i })).toBeInTheDocument();
  });

  it("stops using the network computer when the user picks this Mac and goes back", async () => {
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
      command === "get_active_remote_server" ? Promise.resolve("remote-1") : base?.(command, ...rest),
    );
    const user = userEvent.setup();
    renderOnboarding();
    await openSource(user);
    await waitFor(() => expect(screen.getByRole("radio", { name: /^another computer/i })).toHaveAttribute("aria-checked", "true"));
    await user.click(screen.getByRole("radio", { name: /^on this Mac/i }));
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
    expect(screen.getByRole("radio", { name: "Small English" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("466 MB")).toBeInTheDocument();
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

  it("recommends only a model that speaks the chosen language and names the one it warns about", async () => {
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
      recommended: true,
    } as ModelInfo;
    modelManagement.models.base = {
      ...modelManagement.models["base.en"],
      name: "base",
      display_name: "Base",
      size: 142 * 1024 * 1024,
      downloaded: false,
      accuracy_score: 6,
      speed_score: 4,
      recommended: true,
    } as ModelInfo;
    modelManagement.modelOrder = ["base.en", "small.en", "base"];
    const user = userEvent.setup();
    renderOnboarding();
    await openEngine(user);
    const [first] = screen.getAllByRole("radio");
    expect(first).toHaveAccessibleName("Base");
    expect(first).toHaveTextContent("Recommended");
    expect(screen.getByRole("radio", { name: "Small English" })).not.toHaveTextContent("Recommended");
    expect(screen.getByRole("radio", { name: "Small English" })).toHaveTextContent("not your language");
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
    await user.click(await screen.findByRole("radio", { name: /^Cloud, with your key/i }));
    modelManagement.models["small.en"] = { ...modelManagement.models["small.en"], downloaded: true };
    view.rerender(<OnboardingDesktop onComplete={onCompleteMock} licenseStatus={licensed} modelManagement={modelManagement as never} />);
    await act(async () => {});
    expect(updateSettingsMock).not.toHaveBeenCalledWith(expect.objectContaining({ current_model: "small.en" }));
  });

  it("keeps Continue off with no computer on the network and offers to add one by address", async () => {
    const user = userEvent.setup();
    settingsState.current_model = "";
    renderOnboarding();

    await openSource(user);
    await pickSource(user, /^another computer/i);

    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Add a server by address" }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
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
    await pickSource(user, /^another computer/i);
    expect(screen.getByText(/use only on a trusted network/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();

    await user.click(await screen.findByRole("button", { name: /^use$/i }));

    expect(invokeMock).toHaveBeenCalledWith("set_active_remote_server", {
      serverId: "remote-1",
    });
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(await screen.findByRole("heading", { name: /let voicetypr hear you/i })).toBeInTheDocument();
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
    await pickSource(user, /^Cloud, with your key/i);
    expect(screen.getByText(/your audio goes to the provider you pick/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    for (const provider of ["Soniox", "Deepgram", "OpenAI", "Groq", "Cohere"]) {
      expect(screen.getByText(provider)).toBeInTheDocument();
    }
    await user.click(screen.getByRole("button", { name: "Add API key for Soniox" }));
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
    expect(screen.getByRole("radio", { name: /^press to start, press to stop/i })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByLabelText("Right ⌥")).toBeInTheDocument();
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
    expect(screen.getByRole("radio", { name: /^hold to talk/i })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByLabelText("Right ⌥")).toBeInTheDocument();
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
    "walks the promise, source, setup, Polish, first try, its result, the fork and done on %s",
    async (platform) => {
      Object.assign(platformMock, {
        isMacOS: platform === "macos",
        isWindows: platform === "windows",
      });
      const user = userEvent.setup();
      renderOnboarding();
      expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(/Say it messy\.\s*It lands clean\./);
      expect(screen.queryByRole("progressbar", { name: "Setup progress" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /^back$/i })).not.toBeInTheDocument();
      await reachSetup(user);
      expect(screen.getByRole("progressbar", { name: "Setup progress" })).toHaveAttribute("aria-valuenow", "4");
      expect(screen.getByRole("button", { name: /^back$/i })).toBeInTheDocument();
      if (platform === "macos") {
        expect(screen.getByRole("heading", { name: "Let Voicetypr hear you and type for you" })).toBeInTheDocument();
        expect(screen.getByRole("heading", { name: "Accessibility" })).toBeInTheDocument();
      } else {
        expect(screen.getByRole("heading", { name: "Can Voicetypr hear you?" })).toBeInTheDocument();
        expect(screen.queryByText("Accessibility")).not.toBeInTheDocument();
        expect(screen.getByRole("meter", { name: "Microphone input level" })).toBeInTheDocument();
      }
      await toShortcut(user);
      expect(screen.getByRole("radio", { name: /^press to start, press to stop/i })).toHaveAttribute("aria-checked", "true");
      expect(screen.queryByRole("button", { name: /skip for now/i })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: "Let AI tidy your words" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: "Try it for real" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /skip for now/i }));
      await screen.findByRole("heading", { name: "That’s the essentials" });
      await user.click(screen.getByRole("button", { name: /^set your style and see what else it does/i }));
      await screen.findByRole("heading", { name: "What will you mostly dictate?" });
      expect(screen.getByRole("progressbar", { name: "Setup progress" })).toHaveAttribute("aria-valuenow", "9");
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      for (let step = 0; step < 7; step++) {
        expect(await screen.findByRole("button", { name: "Skip the tour" })).toBeInTheDocument();
        await user.click(screen.getByRole("button", { name: /^next$/i }));
      }
      await screen.findByRole("heading", { name: /welcome back/i });
      expect(screen.getByRole("progressbar", { name: "Setup progress" })).toHaveAttribute("aria-valuenow", "12");
      expect(screen.queryByRole("button", { name: /^back$/i })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Start using Voicetypr" }));
      await waitFor(() => expect(settingsState.onboarding_completed).toBe(true));
      expect(onCompleteMock).toHaveBeenCalledWith(undefined);
    },
  );

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
    await screen.findByRole("heading", { name: "Let AI tidy your words" });
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(await screen.findByText(/Press it again when you’re done/)).toBeInTheDocument();
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
      const begin = await screen.findByRole("button", { name: /^get started/i });
      expect(modelManagement.downloadModel).not.toHaveBeenCalled();
      await user.click(begin);
      await screen.findByRole("heading", { name: /where should your voice become text/i });
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("heading", { name: "Which language do you speak?" });
      expect(modelManagement.downloadModel).not.toHaveBeenCalled();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByText("670 MB")).toBeInTheDocument();
      await waitFor(() => expect(modelManagement.downloadModel).toHaveBeenCalledWith("base.en"));
      modelManagement.downloadProgress = { "base.en": 42 };
      rerender();
      expect(screen.getByRole("progressbar", { name: "Downloading Parakeet" })).toHaveAttribute("aria-valuenow", "42");
      expect(screen.getByText("42%")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Continue while it downloads" }));
      expect(await screen.findByRole("heading", { name: /let voicetypr hear you|can voicetypr hear you/i })).toBeInTheDocument();
      expect(screen.getByText("Parakeet · 42%")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      expect(await screen.findByRole("heading", { name: "Your shortcut" })).toBeInTheDocument();
      expect(screen.getByText("Parakeet · 42%")).toBeInTheDocument();
      await skipUse(user);
      expect(await screen.findByRole("heading", { name: "Almost ready" })).toBeInTheDocument();
      expect(modelManagement.downloadModel).toHaveBeenCalledTimes(1);
      expect(screen.getByText("Parakeet is still downloading. You can try it the moment it’s done.")).toBeInTheDocument();
      expect(screen.getByText("281 of 670 MB")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Try it" })).toBeDisabled();
      expect(screen.queryByRole("button", { name: /retry/i })).not.toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: "Try it for real" })).not.toBeInTheDocument();

      modelManagement.downloadProgress = {};
      Object.assign(modelManagement, { downloadErrors: { "base.en": "error sending request: connection reset" } });
      view.rerender(
        <OnboardingDesktop
          onComplete={onCompleteMock}
          licenseStatus={licensed}
          modelManagement={modelManagement as never}
        />,
      );
      expect(screen.getByRole("alert")).toHaveTextContent("The download stopped.");
      expect(screen.getByText("Parakeet · download stopped")).toBeInTheDocument();
      expect(screen.queryByText(/connection reset/)).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /retry/i }));
      expect(modelManagement.downloadModel).toHaveBeenCalledTimes(2);

      delete (modelManagement as { downloadErrors?: unknown }).downloadErrors;
      modelManagement.models["base.en"] = { ...modelManagement.models["base.en"], downloaded: true };
      rerender();
      expect(await screen.findByText("Parakeet is ready")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Try it" }));
      expect(await screen.findByRole("heading", { name: "Try it for real" })).toBeInTheDocument();
      await pastTry(user);
      expect(await screen.findByRole("heading", { name: /welcome back/i })).toBeInTheDocument();
    },
  );

  it("keeps the step count when a trial starts mid-flow", async () => {
    const user = userEvent.setup();
    const view = renderOnboarding({ licenseStatus: { status: "none" } });
    await openSource(user);
    expect(screen.getByRole("progressbar", { name: "Setup progress" })).toHaveAttribute("aria-valuemax", "13");
    view.rerender(
      <OnboardingDesktop
        onComplete={onCompleteMock}
        licenseStatus={{ status: "trial", trial_days_left: 7 }}
        modelManagement={modelManagement as never}
      />,
    );
    expect(screen.getByRole("progressbar", { name: "Setup progress" })).toHaveAttribute("aria-valuemax", "13");
    expect(screen.getByRole("progressbar", { name: "Setup progress" })).toHaveAttribute("aria-valuenow", "1");
  });

  const reachTry = async (user: User) => {
    await reachSetup(user);
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await skipUse(user);
    await screen.findByRole("heading", { name: "Try it for real" });
  };
  const emit = (event: string, payload: unknown) =>
    act(() => {
      for (const handler of eventListeners.get(event) ?? []) handler({ payload });
    });

  const tryCommands = (allowance: unknown = { kind: "tries", left: 10 }, heard = "um so Friday at three") => {
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
      command === "get_onboarding_try" ? Promise.resolve(allowance)
        : command === "start_onboarding_try" ? Promise.resolve({ id: "fffffffffffffffe", left: 9 })
        : command === "stop_onboarding_try" ? Promise.resolve({ heard })
        : base?.(command, ...rest),
    );
  };
  const neverRecords = () =>
    expect(invokeMock.mock.calls.some(([command]) => command === "start_recording" || command === "stop_recording")).toBe(false);

  it("shows the engine's own text on its own screen, then the fork", async () => {
    tryCommands();
    const user = userEvent.setup();
    renderOnboarding({ licenseStatus: { status: "none" } });
    await reachTry(user);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("arm_onboarding_try", { armed: true }));
    expect(screen.queryByRole("button", { name: /click here to talk|^stop$/i })).not.toBeInTheDocument();
    emit("onboarding-try-key", true);
    expect(await screen.findByText("Listening…")).toBeInTheDocument();
    emit("onboarding-try-key", true);
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("stop_onboarding_try", { id: "fffffffffffffffe" }));
    expect(await screen.findByRole("heading", { name: "That’s you, word for word" })).toBeInTheDocument();
    expect(screen.getByText("You said")).toBeInTheDocument();
    expect(screen.getByText("um so Friday at three")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /try again/i })).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("arm_onboarding_try", { armed: false });
    neverRecords();
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(await screen.findByRole("heading", { name: "That’s the essentials" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^done for now/i }));
    expect(await screen.findByRole("heading", { name: "Keep talking like that, everywhere" })).toBeInTheDocument();
    expect(screen.getByText("You said it. Voicetypr typed it.")).toBeInTheDocument();
    expect(screen.getByText("um so Friday at three")).toBeInTheDocument();
    expect(screen.queryByText("Example of Polish")).not.toBeInTheDocument();
  });

  it("shows the really cleaned text on the result and as Buy's proof when Polish cleaned the try", async () => {
    polishStatus = { ...BUILTIN_READY, builtin: "first_try" };
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
      command === "get_onboarding_try" ? Promise.resolve({ kind: "tries", left: 10 })
        : command === "start_onboarding_try" ? Promise.resolve({ id: "fffffffffffffffe", left: 9 })
        : command === "stop_onboarding_try"
          ? Promise.resolve({ heard: "um so thursday no friday at three", cleaned: "Friday at three." })
          : base?.(command, ...rest),
    );
    const user = userEvent.setup();
    renderOnboarding({ licenseStatus: { status: "none" } });
    await reachTry(user);
    await waitFor(() => expect(eventListeners.get("onboarding-try-key")?.size).toBe(1));
    emit("onboarding-try-key", true);
    await screen.findByText("Listening…");
    emit("onboarding-try-key", true);
    expect(await screen.findByRole("heading", { name: "That’s you, cleaned up" })).toBeInTheDocument();
    expect(screen.getByText("Voicetypr cleaned it")).toBeInTheDocument();
    expect(screen.getByText("Friday at three.")).toBeInTheDocument();
    neverRecords();
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await user.click(await screen.findByRole("button", { name: /^done for now/i }));
    expect(await screen.findByRole("heading", { name: "Keep talking like that, everywhere" })).toBeInTheDocument();
    expect(screen.getByText("Friday at three.")).toBeInTheDocument();
    expect(screen.queryByText("Example of Polish")).not.toBeInTheDocument();
  });

  it("offers a click to talk on macOS when Accessibility was skipped", async () => {
    permissionState.accessibility = false;
    try {
      tryCommands();
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" } });
      await reachTry(user);
      await user.click(await screen.findByRole("button", { name: "Or click here to talk" }));
      expect(await screen.findByText("Listening…")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Or click here to talk" }));
      expect(await screen.findByRole("heading", { name: "That’s you, word for word" })).toBeInTheDocument();
      expect(screen.getByText("um so Friday at three")).toBeInTheDocument();
      neverRecords();
    } finally {
      permissionState.accessibility = true;
    }
  });

  it("says Polish didn’t run on the try even when Polish is on", async () => {
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
      command === "get_ai_settings" ? Promise.resolve({ enabled: true })
        : command === "polish_can_run" ? Promise.resolve(true) : base?.(command, ...rest),
    );
    tryCommands(undefined, "um so let's move the sync to thursday no friday at three");
    const user = userEvent.setup();
    renderOnboarding();
    await reachTry(user);
    await waitFor(() => expect(eventListeners.get("onboarding-try-key")?.size).toBe(1));
    emit("onboarding-try-key", true);
    await screen.findByText("Listening…");
    emit("onboarding-try-key", true);
    expect(await screen.findByText("um so let's move the sync to thursday no friday at three")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "That’s you, word for word" })).toBeInTheDocument();
    expect(screen.getByText(/Polish didn’t run on this try, so there was no AI clean-up/)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "That’s you, cleaned up" })).not.toBeInTheDocument();
  });

  it("starts and stops the try from the user's shortcut while the step is on screen", async () => {
    tryCommands();
    const user = userEvent.setup();
    renderOnboarding({ licenseStatus: { status: "none" } });
    await reachTry(user);
    await waitFor(() => expect(eventListeners.get("onboarding-try-key")?.size).toBe(1));
    emit("onboarding-try-key", true);
    await screen.findByText("Listening…");
    emit("onboarding-try-key", false);
    expect(invokeMock).not.toHaveBeenCalledWith("stop_onboarding_try", expect.anything());
    emit("onboarding-try-key", true);
    expect(await screen.findByText("um so Friday at three")).toBeInTheDocument();
    expect(invokeMock.mock.calls.filter(([command]) => command === "start_onboarding_try")).toHaveLength(1);
    neverRecords();
  });

  it("shows nothing from a try that ended when the window lost focus", async () => {
    tryCommands();
    let answer: ((value: { heard: string }) => void) | undefined;
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
      command === "stop_onboarding_try" ? new Promise((resolve) => { answer = resolve; }) : base?.(command, ...rest),
    );
    const user = userEvent.setup();
    renderOnboarding({ licenseStatus: { status: "none" } });
    await reachTry(user);
    await waitFor(() => expect(eventListeners.get("onboarding-try-key")?.size).toBe(1));
    emit("onboarding-try-key", true);
    await screen.findByText("Listening…");
    emit("onboarding-try-key", true);
    await screen.findByText("Transcribing…");
    act(() => { window.dispatchEvent(new Event("blur")); });
    await act(async () => { answer?.({ heard: "said while away" }); });
    expect(screen.queryByText("said while away")).not.toBeInTheDocument();
    expect(screen.queryByText("Transcribing…")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Try it for real" })).toBeInTheDocument();
  });

  it("says this device's tries are used up and moves on to the fork", async () => {
    tryCommands({ kind: "usedUp" });
    const user = userEvent.setup();
    renderOnboarding({ licenseStatus: { status: "none" } });
    await reachSetup(user);
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await skipUse(user);
    expect(await screen.findByRole("heading", { name: "Your first tries are used up" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Try it for real" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    expect(await screen.findByRole("heading", { name: "That’s the essentials" })).toBeInTheDocument();
    expect(invokeMock.mock.calls.some(([command]) => command === "start_onboarding_try")).toBe(false);
    neverRecords();
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

  it("asks for the microphone on Permissions and holds Continue until it is allowed on macOS", async () => {
    permissionState.microphone = "not_determined";
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    expect(screen.getByRole("button", { name: /^continue$/i })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Allow microphone" }));
    expect(permissionState.requestMicrophone).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalledWith("start_onboarding_try");
  });

  it("explains a microphone switched off before the try and opens its settings", async () => {
    const user = userEvent.setup();
    const view = renderOnboarding();
    await reachTry(user);
    permissionState.microphone = "denied";
    view.rerender(
      <OnboardingDesktop onComplete={onCompleteMock} licenseStatus={licensed} modelManagement={modelManagement as never} />,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(/The microphone is off for Voicetypr/);
    await user.click(screen.getByRole("button", { name: "Open System Settings" }));
    expect(invokeMock).toHaveBeenCalledWith("open_microphone_settings");
  });

  it("lets Accessibility be skipped once the microphone is allowed", async () => {
    permissionState.accessibility = false;
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    expect(screen.getByRole("button", { name: "Enable Accessibility" })).toBeInTheDocument();
    expect(screen.getByText("Allowed")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Skip Accessibility, I’ll paste myself" }));
    await screen.findByRole("heading", { name: "Your shortcut" });
    await skipUse(user);
    await pastTry(user);
    await screen.findByRole("heading", { name: /welcome back/i });
  });

  it("hides the Accessibility skip once Accessibility is allowed", async () => {
    const user = userEvent.setup();
    renderOnboarding();
    await reachSetup(user);
    expect(screen.getAllByText("Allowed")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: /skip accessibility/i })).not.toBeInTheDocument();
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
    await screen.findByRole("heading", { name: "Let AI tidy your words" });
    await user.click(screen.getByRole("button", { name: /^continue$/i }));
    await screen.findByRole("heading", { name: "Try it for real" });
    expect(
      invokeMock.mock.calls.filter(([command]) => command === "set_global_shortcut"),
    ).toHaveLength(1);
  });

  describe("buy and trial steps", () => {
    const fillContact = async (user: User) => {
      await user.type(await screen.findByLabelText("Name"), "Priya Raman");
      await user.type(screen.getByLabelText("Email"), "priya@example.com");
    };
    const BUY_HEADING = "Keep talking like that, everywhere";
    // The paid screens come last, after the first try and the fork.
    const reachPaid = async (user: User) => {
      await reachSetup(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await skipUse(user);
      await pastTry(user);
    };
    const reachBuy = async (user: User) => {
      await reachPaid(user);
      await screen.findByRole("heading", { name: BUY_HEADING });
    };
    const reachTrial = async (user: User) => {
      await reachBuy(user);
      await user.click(screen.getByRole("button", { name: "Start 7-day trial · No card" }));
      await screen.findByRole("heading", { name: "Start your 7-day trial" });
      await fillContact(user);
    };

    it("puts Buy after the first try, and asks for a name and email only after Start 7-day trial", async () => {
      const start = vi.fn().mockResolvedValue(null);
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" }, onStartTrial: start });
      await openSource(user);
      expect(screen.queryByRole("button", { name: /start 7-day trial/i })).not.toBeInTheDocument();
      await throughLanguage(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("heading", { name: /let voicetypr hear you/i });
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await skipUse(user);
      await pastTry(user);
      await screen.findByRole("heading", { name: BUY_HEADING });
      expect(screen.getByRole("progressbar", { name: "Setup progress" })).toHaveAttribute("aria-valuenow", "10");
      expect(screen.getByRole("button", { name: /^buy now/i })).toBeInTheDocument();
      expect(screen.getByText("Example of Polish")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /decide later/i })).not.toBeInTheDocument();
      expect(screen.queryByText(/upgrade/i)).not.toBeInTheDocument();
      expect(screen.queryByLabelText("Email")).not.toBeInTheDocument();
      expect(screen.queryByRole("checkbox", { name: /share anonymous crash reports/i })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Start 7-day trial · No card" }));
      await screen.findByRole("heading", { name: "Start your 7-day trial" });
      await fillContact(user);
      await user.click(screen.getByRole("button", { name: "Start trial" }));
      expect(start).toHaveBeenCalledWith("Priya Raman", "priya@example.com");
      expect(await screen.findByRole("heading", { name: "You’re set, Priya" })).toBeInTheDocument();
      expect(invokeMock.mock.calls.some(([command]) => command === "start_recording")).toBe(false);
    });

    it("says Voicetypr couldn’t be reached, never that the trial ended, when the start fails", async () => {
      const start = vi.fn().mockResolvedValue("trial_unreachable");
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: null, onStartTrial: start });
      await reachTrial(user);
      await user.click(screen.getByRole("button", { name: "Start trial" }));
      expect(await screen.findByRole("heading", { name: "Couldn’t reach Voicetypr" })).toBeInTheDocument();
      expect(screen.queryByText(/expired|trial ended|trial has ended/i)).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: /i have a key/i })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /try again/i }));
      expect(start).toHaveBeenCalledTimes(2);
      expect(start).toHaveBeenLastCalledWith("Priya Raman", "priya@example.com");
      await user.click(screen.getByRole("button", { name: /^back$/i }));
      expect(await screen.findByRole("heading", { name: "Start your 7-day trial" })).toBeInTheDocument();
      expect(screen.getByLabelText("Email")).toHaveValue("priya@example.com");
      await user.click(screen.getByRole("button", { name: /^back$/i }));
      expect(await screen.findByRole("heading", { name: BUY_HEADING })).toBeInTheDocument();
    });

    it.each([
      ["trial_rate_limited", /Too many attempts, so your trial hasn’t started/],
      ["trial_email_used", /This email has already been used for a trial/],
    ] as const)("keeps the user on the name and email page on %s", async (error, message) => {
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" }, onStartTrial: vi.fn().mockResolvedValue(error) });
      await reachTrial(user);
      await user.click(screen.getByRole("button", { name: "Start trial" }));
      expect(await screen.findByRole("alert")).toHaveTextContent(message);
      expect(screen.getByRole("heading", { name: "Start your 7-day trial" })).toBeInTheDocument();
      expect(screen.getByLabelText("Email")).toHaveValue("priya@example.com");
      expect(screen.queryByText(/trial ended|trial has ended/i)).not.toBeInTheDocument();
    });

    it("skips Buy when this device already has a license", async () => {
      const user = userEvent.setup();
      renderOnboarding({ onStartTrial: vi.fn() });
      await finishSetup(user);
      expect(await screen.findByRole("heading", { name: "Welcome back" })).toBeInTheDocument();
      expect(screen.getByText(/Your licence is active\./)).toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: BUY_HEADING })).not.toBeInTheDocument();
    });

    it.each([
      ["a new device", { status: "none" }, BUY_HEADING],
      ["a device whose trial ended", { status: "expired" }, "Your free week is over"],
    ] as const)("opens pricing and then the key screen on Buy now on %s", async (_name, licenseStatus, heading) => {
      const openPricing = vi.fn().mockResolvedValue(undefined);
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus, onOpenPricing: openPricing });
      await reachPaid(user);
      await screen.findByRole("heading", { name: heading });
      await user.click(screen.getByRole("button", { name: /^buy now/i }));
      expect(openPricing).toHaveBeenCalledTimes(1);
      expect(await screen.findByRole("heading", { name: "Enter your licence key" })).toBeInTheDocument();
      expect(screen.getByRole("textbox", { name: "Licence key" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /buy on voicetypr\.com/i }));
      expect(openPricing).toHaveBeenCalledTimes(2);
      await user.click(screen.getByRole("button", { name: /^back$/i }));
      expect(await screen.findByRole("heading", { name: heading })).toBeInTheDocument();
    });

    it("shows a key error in plain words, then goes straight to Done once a key activates", async () => {
      const activate = vi.fn()
        .mockResolvedValueOnce("That key didn’t work. Check it against your receipt email.")
        .mockResolvedValueOnce(null);
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" }, onActivateLicense: activate });
      await reachBuy(user);
      await user.click(screen.getByRole("button", { name: /i have a key/i }));
      expect(screen.getByRole("button", { name: "Activate" })).toBeDisabled();
      await user.type(screen.getByRole("textbox", { name: "Licence key" }), "VT-BAD");
      await user.click(screen.getByRole("button", { name: "Activate" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("That key didn’t work. Check it against your receipt email.");
      expect(screen.getByRole("heading", { name: "Enter your licence key" })).toBeInTheDocument();
      await user.clear(screen.getByRole("textbox", { name: "Licence key" }));
      await user.type(screen.getByRole("textbox", { name: "Licence key" }), "VT-GOOD");
      await user.click(screen.getByRole("button", { name: "Activate" }));
      expect(activate).toHaveBeenLastCalledWith("VT-GOOD");
      expect(await screen.findByRole("heading", { name: "Voicetypr is yours" })).toBeInTheDocument();
    });

    it("signs Buy from Moinul without a reply-time promise", async () => {
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" }, onStartTrial: vi.fn() });
      await reachBuy(user);
      expect(screen.getByText("Made by Moinul. I read every email.")).toBeInTheDocument();
      expect(screen.queryByText(/within|Made by Moin\b/)).not.toBeInTheDocument();
    });

    it("offers only Buy now and a key when this device's trial has ended", async () => {
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "expired" }, onStartTrial: vi.fn() });
      await reachPaid(user);
      expect(await screen.findByRole("heading", { name: "Your free week is over" })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /start 7-day trial/i })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: /^buy now/i })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /i have a key/i })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /^back$/i })).not.toBeInTheDocument();
      expect(screen.queryByText(/upgrade|trial ended/i)).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /i have a key/i }));
      expect(await screen.findByRole("textbox", { name: "Licence key" })).toBeInTheDocument();
    });

    it("offers a recheck instead of Buy when the licence check failed, without saying the trial ended", async () => {
      const recheck = vi.fn().mockResolvedValue(undefined);
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" }, licenseCheckFailed: true, onRecheckLicense: recheck });
      await reachPaid(user);
      expect(await screen.findByRole("heading", { name: "Couldn’t check your licence" })).toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: BUY_HEADING })).not.toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: "Your free week is over" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /^back$/i })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /recheck/i }));
      expect(recheck).toHaveBeenCalledTimes(1);
    });

    it("reopens at Buy after the window closed there, and forgets it once onboarding completes", async () => {
      const user = userEvent.setup();
      const first = renderOnboarding({ licenseStatus: { status: "none" }, onStartTrial: vi.fn().mockResolvedValue(null) });
      await reachBuy(user);
      expect(localStorage.getItem("onboarding_reached_buy")).not.toBeNull();
      first.unmount();

      renderOnboarding({ licenseStatus: { status: "none" }, onStartTrial: vi.fn().mockResolvedValue(null) });
      expect(await screen.findByRole("heading", { name: BUY_HEADING })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Start 7-day trial · No card" }));
      await fillContact(user);
      await user.click(screen.getByRole("button", { name: "Start trial" }));
      await user.click(await screen.findByRole("button", { name: "Start using Voicetypr" }));
      await waitFor(() => expect(onCompleteMock).toHaveBeenCalledTimes(1));
      expect(localStorage.getItem("onboarding_reached_buy")).toBeNull();
    });
  });

  describe("the tour", () => {
    const reachTour = async (user: User) => {
      await reachSetup(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await skipUse(user);
      await user.click(await screen.findByRole("button", { name: /skip for now/i }));
      await user.click(await screen.findByRole("button", { name: /^set your style and see what else it does/i }));
      await screen.findByRole("heading", { name: "What will you mostly dictate?" });
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await screen.findByRole("heading", { name: "Fix a word once" });
    };

    it("opens the section the tour remembered once setup is done", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachTour(user);
      await user.click(screen.getByRole("button", { name: "Open Dictionary" }));
      expect(screen.getByRole("button", { name: "Opens after setup" })).toHaveAttribute("aria-pressed", "true");
      await user.click(screen.getByRole("button", { name: /^next$/i }));
      expect(await screen.findByRole("heading", { name: "Say “new line”" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: /^back$/i }));
      expect(await screen.findByRole("button", { name: "Opens after setup" })).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Skip the tour" }));
      await screen.findByRole("heading", { name: "Welcome back" });
      await user.click(screen.getByRole("button", { name: "Start using Voicetypr" }));
      await waitFor(() => expect(onCompleteMock).toHaveBeenCalledWith("dictionary"));
    });

    it("goes to Buy after the tour on a device without a licence", async () => {
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" } });
      await reachTour(user);
      await user.click(screen.getByRole("button", { name: "Skip the tour" }));
      expect(await screen.findByRole("heading", { name: "Keep talking like that, everywhere" })).toBeInTheDocument();
    });
  });

  describe("the Polish switch", () => {
    it("shows Polish on by default, says nothing is kept, and turns it off for the next dictation", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachPolish(user);
      const toggle = screen.getByRole("switch", { name: "Clean up what I say" });
      expect(toggle).toBeChecked();
      expect(screen.getByRole("status")).toHaveTextContent("Your words go to Voicetypr’s server to be cleaned, then straight back. Nothing is kept.");
      expect(screen.queryByRole("heading", { name: "What will you mostly dictate?" })).not.toBeInTheDocument();
      await user.click(toggle);
      // The same saved setting Settings → Polish writes, which the next dictation reads.
      expect(updateSettingsMock).toHaveBeenCalledWith({ builtin_polish_enabled: false });
      expect(await screen.findByRole("status")).toHaveTextContent("No AI cleanup. Your Dictionary and voice commands still apply.");
    });

    it("keeps Polish off for an existing install that never chose it", async () => {
      delete (settingsState as Record<string, unknown>).builtin_polish_enabled;
      const user = userEvent.setup();
      renderOnboarding();
      await reachPolish(user);
      expect(screen.getByRole("switch", { name: "Clean up what I say" })).not.toBeChecked();
      expect(updateSettingsMock).not.toHaveBeenCalledWith({ builtin_polish_enabled: true });
    });

    it.each([
      ["not_entitled", "Not available on this device: built-in Polish needs a trial started with your email, or a license."],
      ["unreachable", "Voicetypr’s server can’t take Polish right now (offline or busy), so nothing is sent and you get exactly what you said."],
    ])("never shows built-in Polish as on when the server says %s", async (builtin, line) => {
      polishStatus = { ...BUILTIN_READY, builtin };
      const user = userEvent.setup();
      renderOnboarding();
      await reachPolish(user);
      expect(await screen.findByText(line)).toBeInTheDocument();
      const toggle = screen.getByRole("switch", { name: "Clean up what I say" });
      expect(toggle).not.toBeChecked();
      expect(toggle).toHaveAttribute("aria-disabled", "true");
    });

    it("shows the saved choice while Voicetypr's server is still being asked, never an off it can't change", async () => {
      const base = invokeMock.getMockImplementation();
      invokeMock.mockImplementation((command: string, ...rest: unknown[]) =>
        command === "polish_status" ? new Promise(() => undefined) : base?.(command, ...rest),
      );
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" } });
      await reachPolish(user);
      const toggle = screen.getByRole("switch", { name: "Clean up what I say" });
      expect(toggle).toBeChecked();
      expect(toggle).toBeEnabled();
    });

    it("offers the switch before any trial, on the First try's own allowance", async () => {
      polishStatus = { ...BUILTIN_READY, builtin: "first_try" };
      const user = userEvent.setup();
      renderOnboarding({ licenseStatus: { status: "none" } });
      await reachPolish(user);
      const toggle = screen.getByRole("switch", { name: "Clean up what I say" });
      expect(toggle).toBeChecked();
      expect(toggle).toBeEnabled();
    });

    it("says where Polish can be turned off and keeps the screen for a user's own provider", async () => {
      polishStatus = { will_run: true, own_provider_ready: true, own_provider_enabled: true, builtin_enabled: false, builtin: "own_provider" };
      const user = userEvent.setup();
      renderOnboarding();
      await reachPolish(user);
      expect(screen.getByRole("status")).toHaveTextContent("Polish is on and set up with your own AI provider.");
      expect(screen.queryByRole("switch")).not.toBeInTheDocument();
    });

    it("opens Settings → Polish after setup for a user who wants their own provider", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachPolish(user);
      await user.click(screen.getByRole("button", { name: "Use my own AI provider" }));
      expect(screen.getByRole("button", { name: "Settings → Polish opens after setup" })).toHaveAttribute("aria-pressed", "true");
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await user.click(await screen.findByRole("button", { name: /skip for now/i }));
      await user.click(await screen.findByRole("button", { name: /^done for now/i }));
      await user.click(await screen.findByRole("button", { name: "Start using Voicetypr" }));
      await waitFor(() => expect(onCompleteMock).toHaveBeenCalledWith("polish"));
    });

    it("says so when the switch couldn't be saved", async () => {
      const toastError = vi.spyOn(toast, "error");
      const user = userEvent.setup();
      renderOnboarding();
      await reachPolish(user);
      updateSettingsMock.mockImplementationOnce(() => Promise.reject(new Error("disk")));
      await user.click(screen.getByRole("switch", { name: "Clean up what I say" }));
      await waitFor(() => expect(toastError).toHaveBeenCalledWith("Couldn’t save the Polish switch. Try again."));
    });
  });

  describe("what you mostly dictate", () => {
    const reachUse = async (user: User) => {
      await reachPolish(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await user.click(await screen.findByRole("button", { name: /skip for now/i }));
      await user.click(await screen.findByRole("button", { name: /^set your style and see what else it does/i }));
      await screen.findByRole("heading", { name: "What will you mostly dictate?" });
    };
    const reachTourStart = () => screen.findByRole("heading", { name: "Fix a word once" });

    it("comes first on the deeper path only when Polish is on", async () => {
      Object.assign(settingsState, { builtin_polish_enabled: false });
      const user = userEvent.setup();
      renderOnboarding();
      await reachPolish(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await user.click(await screen.findByRole("button", { name: /skip for now/i }));
      await user.click(await screen.findByRole("button", { name: /^set your style and see what else it does/i }));
      await reachTourStart();
      expect(screen.queryByRole("heading", { name: "What will you mostly dictate?" })).not.toBeInTheDocument();
    });

    it("applies the pick at once when built-in Polish is on, then starts the tour", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: "Messages and email" }));
      expect(screen.getByRole("button", { name: "Messages and email" })).toHaveAttribute("aria-pressed", "true");
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await reachTourStart();
      expect(invokeMock).toHaveBeenCalledWith("update_enhancement_options", { options: { preset: "Message" } });
      // Kept too, so a provider of the user's own starts in the same style.
      expect(updateSettingsMock).toHaveBeenCalledWith({ starting_polish_style: "Message" });
    });

    it("waits to apply the pick when the user's own provider is set up but off", async () => {
      polishStatus = { will_run: false, own_provider_ready: true, own_provider_enabled: false, builtin_enabled: false, builtin: "own_provider" };
      Object.assign(settingsState, { builtin_polish_enabled: false });
      const user = userEvent.setup();
      renderOnboarding();
      await reachPolish(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await user.click(await screen.findByRole("button", { name: /skip for now/i }));
      await user.click(await screen.findByRole("button", { name: /^set your style and see what else it does/i }));
      // Own provider off: Polish isn't on, so the deeper path starts with the tour.
      await reachTourStart();
    });

    it("sets the style right away when the user's own provider is on", async () => {
      polishStatus = { will_run: true, own_provider_ready: true, own_provider_enabled: true, builtin_enabled: false, builtin: "own_provider" };
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: "Code and tech notes" }));
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await reachTourStart();
      expect(invokeMock).toHaveBeenCalledWith("update_enhancement_options", { options: { preset: "Code" } });
      expect(updateSettingsMock).not.toHaveBeenCalledWith({ starting_polish_style: "Code" });
    });

    it("continues without a pick, keeping today's default style", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await reachTourStart();
      expect(updateSettingsMock).not.toHaveBeenCalledWith(expect.objectContaining({ starting_polish_style: expect.anything() }));
      expect(updateSettingsMock).not.toHaveBeenCalledWith({ starting_polish_style: null });
      expect(invokeMock).not.toHaveBeenCalledWith("update_enhancement_options", expect.anything());
    });

    it("goes back from the tour to the pick, and from the pick to the fork", async () => {
      const user = userEvent.setup();
      renderOnboarding();
      await reachUse(user);
      await user.click(screen.getByRole("button", { name: /^continue$/i }));
      await reachTourStart();
      await user.click(screen.getByRole("button", { name: /^back$/i }));
      await screen.findByRole("heading", { name: "What will you mostly dictate?" });
      await user.click(screen.getByRole("button", { name: /^back$/i }));
      await screen.findByRole("heading", { name: "That’s the essentials" });
    });
  });
});
