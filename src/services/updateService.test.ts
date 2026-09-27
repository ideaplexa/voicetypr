import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";

// Mock Tauri plugins before importing the module under test

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  sendNotification: vi.fn(),
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: {
    info: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
    dismiss: vi.fn(),
  },
}));

import { UpdateService, type AppUpdateInfo } from "./updateService";
import { sendNotification, isPermissionGranted } from "@tauri-apps/plugin-notification";
import type { DistributionInfo } from "@/types/distribution";
import type { AppSettings } from "@/types";

const JUST_UPDATED_KEY = "just_updated_version";
const storage = new Map<string, string>();

const testSettings = (overrides: Partial<AppSettings> = {}): AppSettings => ({
  hotkey: "CmdOrCtrl+Shift+Space",
  current_model: "tiny.en",
  speech_language: "en",
  theme: "system",
  ...overrides,
});

Object.defineProperty(window, "localStorage", {
  configurable: true,
  value: {
    getItem: vi.fn((key: string) => storage.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      storage.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      storage.delete(key);
    }),
    clear: vi.fn(() => {
      storage.clear();
    }),
  },
});

function mockDirectDistribution(update: AppUpdateInfo | null = null): void {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_distribution_info") {
      return {
        channel: "direct",
        is_store_install: false,
        package_family_name: null,
      };
    }

    if (command === "get_current_recording_state") {
      return { state: "idle" };
    }

    if (command === "check_for_app_update") {
      return update;
    }

    if (command === "install_app_update") {
      return undefined;
    }

    return undefined;
  });
}
describe("UpdateService version marker", () => {
  let service: UpdateService;

  beforeEach(() => {
    storage.clear();
    vi.clearAllMocks();
    mockDirectDistribution();
    // Get a fresh instance per test to reset internal state
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    service = UpdateService.getInstance();
  });

  it("stores version after update install", () => {
    const version = "1.12.1";
    localStorage.setItem(JUST_UPDATED_KEY, version);

    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBe(version);
  });

  it("getJustUpdatedVersion returns and clears marker (one-shot)", () => {
    const version = "2.0.0";
    localStorage.setItem(JUST_UPDATED_KEY, version);

    const result = service.getJustUpdatedVersion();

    expect(result).toBe("2.0.0");
    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBeNull();

    // Second call returns null — marker was consumed
    const result2 = service.getJustUpdatedVersion();
    expect(result2).toBeNull();
  });

  it("returns null when no update marker exists", () => {
    const result = service.getJustUpdatedVersion();

    expect(result).toBeNull();
  });

  it("multiple stores only keep the latest version", () => {
    localStorage.setItem(JUST_UPDATED_KEY, "1.0.0");
    localStorage.setItem(JUST_UPDATED_KEY, "1.12.1");
    localStorage.setItem(JUST_UPDATED_KEY, "2.0.0");

    const result = service.getJustUpdatedVersion();
    expect(result).toBe("2.0.0");
  });

  it("version marker survives simulated crash (persists in localStorage)", () => {
    const version = "1.12.1";
    localStorage.setItem(JUST_UPDATED_KEY, version);

    // Simulate app crash: create a fresh service instance
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    const freshService = UpdateService.getInstance();

    const result = freshService.getJustUpdatedVersion();
    expect(result).toBe("1.12.1");
  });
});

describe("UpdateService update checks", () => {
  let service: UpdateService;

  beforeEach(() => {
    storage.clear();
    vi.clearAllMocks();
    mockDirectDistribution();
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    service = UpdateService.getInstance();
  });

  it("initializes background checks by default without installing updates", async () => {
    await service.initialize(testSettings());

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "check_for_app_update"),
    ).toHaveLength(1);
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "install_app_update"),
    ).toHaveLength(0);
  });

  it("background checks notify without installing updates", async () => {
    vi.mocked(isPermissionGranted).mockResolvedValue(true);
    mockDirectDistribution({
      version: "2.0.0",
      body: "Release notes",
      channel: "stable",
    });

    await service.initialize(testSettings({ check_updates_automatically: true }));

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "check_for_app_update"),
    ).toHaveLength(1);
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "install_app_update"),
    ).toHaveLength(0);
    expect(toast.info).toHaveBeenCalledWith(
      "Update 2.0.0 is available. Open Settings to install it.",
    );
    expect(sendNotification).toHaveBeenCalledWith({
      title: "Update Available",
      body: "Voicetypr 2.0.0 is ready to install from Settings.",
    });
  });

  it("manual checks still ask before installing", async () => {
    mockDirectDistribution({
      version: "2.0.0-beta.1",
      body: "Beta notes",
      channel: "beta",
    });
    vi.mocked(ask).mockResolvedValue(false);

    await service.checkForUpdatesManually();

    expect(ask).toHaveBeenCalled();
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "install_app_update"),
    ).toHaveLength(0);
  });

  it("installs the exact version confirmed by the user", async () => {
    mockDirectDistribution({
      version: "2.0.0-beta.2",
      body: "Beta notes",
      channel: "beta",
    });
    vi.mocked(ask).mockResolvedValue(true);

    await service.checkForUpdatesManually();

    expect(invoke).toHaveBeenCalledWith("install_app_update", {
      expectedVersion: "2.0.0-beta.2",
    });
  });

  it("preserves the update marker when automatic relaunch fails", async () => {
    mockDirectDistribution({
      version: "2.0.0-beta.3",
      body: "Beta notes",
      channel: "beta",
    });
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(relaunch).mockRejectedValue(new Error("relaunch failed"));

    await service.checkForUpdatesManually();
    expect(relaunch).toHaveBeenCalledTimes(1);

    expect(invoke).toHaveBeenCalledWith("install_app_update", {
      expectedVersion: "2.0.0-beta.3",
    });
    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBe("2.0.0-beta.3");
    expect(service.getJustUpdatedVersion()).toBeNull();
    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBe("2.0.0-beta.3");

    // A fresh service instance represents the post-restart process and consumes the marker.
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    const freshService = UpdateService.getInstance();
    expect(freshService.getJustUpdatedVersion()).toBe("2.0.0-beta.3");
    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBeNull();
  });

  it("deduplicates concurrent distribution info requests", async () => {
    let resolveDistribution!: (info: DistributionInfo) => void;
    const distributionInfoPromise = new Promise<DistributionInfo>((resolve) => {
      resolveDistribution = resolve;
    });

    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_distribution_info") {
        return distributionInfoPromise;
      }

      return { state: "idle" };
    });

    const settings: AppSettings = {
      hotkey: "CommandOrControl+Shift+Space",
      current_model: "base.en",
      speech_language: "en",
      theme: "system",
      check_updates_automatically: false,
    };
    const first = service.initialize(settings);
    const second = service.initialize(settings);

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_distribution_info"),
    ).toHaveLength(1);

    resolveDistribution({
      channel: "direct",
      is_store_install: false,
      package_family_name: null,
    });

    await Promise.all([first, second]);
  });

  it("holds update-check lock while distribution info is pending", async () => {
    let resolveDistribution!: (info: DistributionInfo) => void;
    const distributionInfoPromise = new Promise<DistributionInfo>((resolve) => {
      resolveDistribution = resolve;
    });

    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_distribution_info") {
        return distributionInfoPromise;
      }

      if (command === "check_for_app_update") {
        return null;
      }

      return { state: "idle" };
    });

    const backgroundCheck = service.checkForUpdatesInBackground();
    const manualCheck = service.checkForUpdatesManually();

    expect(toast.info).toHaveBeenCalledWith("Update check already in progress");
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "check_for_app_update"),
    ).toHaveLength(0);

    resolveDistribution({
      channel: "direct",
      is_store_install: false,
      package_family_name: null,
    });

    await Promise.all([backgroundCheck, manualCheck]);

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "check_for_app_update"),
    ).toHaveLength(1);
  });

  it("skips direct updater checks for Microsoft Store installs", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_distribution_info") {
        return {
          channel: "store_msix",
          is_store_install: true,
          package_family_name: "Ideaplexa.Voicetypr_12345",
        };
      }

      return { state: "idle" };
    });

    await service.initialize({
      hotkey: "CommandOrControl+Shift+Space",
      current_model: "base.en",
      speech_language: "en",
      theme: "system",
    });

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "check_for_app_update"),
    ).toHaveLength(0);

    await service.checkForUpdatesManually();

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "check_for_app_update"),
    ).toHaveLength(0);
    expect(toast.info).toHaveBeenCalledWith("Updates are handled by Microsoft Store");

    localStorage.setItem(JUST_UPDATED_KEY, "1.12.5");
    expect(service.getJustUpdatedVersion()).toBeNull();
    expect(localStorage.getItem(JUST_UPDATED_KEY)).toBeNull();
  });
});
