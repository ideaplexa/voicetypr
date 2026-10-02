import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { SettingsTab } from "./SettingsTab";
import { createFixtures } from "@/ui-preview/fixtures";
import type { AppSettings } from "@/types";

const state = vi.hoisted(() => ({ settings: null as AppSettings | null }));
const platform = vi.hoisted(() => ({ isMacOS: true, isWindows: false, isLinux: false }));
const updateSettings = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ settings: state.settings, updateSettings }),
}));
vi.mock("@/contexts/ReadinessContext", () => ({
  useReadiness: () => ({
    hasAccessibilityPermission: true,
    hasMicrophonePermission: true,
    isLoading: false,
    requestAccessibilityPermission: vi.fn(),
    requestMicrophonePermission: vi.fn(),
    checkAccessibilityPermission: vi.fn(),
    checkMicrophonePermission: vi.fn(),
  }),
}));
vi.mock("@/lib/platform", () => platform);
vi.mock("@/hooks/useTauriEvent", () => ({ useTauriEvent: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("2.1.0") }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn().mockResolvedValue(false) }));
vi.mock("@/services/updateService", () => ({
  updateService: { checkForUpdatesManually: vi.fn().mockResolvedValue(undefined) },
}));

describe("Settings pane controls", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    platform.isMacOS = true;
    platform.isWindows = false;
    const fixtures = createFixtures({ platform: "macos", theme: "light", empty: false });
    state.settings = fixtures.get_settings as AppSettings;
    vi.mocked(invoke).mockImplementation((command: string) =>
      Promise.resolve(command === "set_autostart" ? true : fixtures[command]),
    );
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockResolvedValue(undefined) },
    });
  });

  it("renders General's real appearance, startup, update and tray controls", async () => {
    const view = render(<SettingsTab pane="general" onPaneChange={vi.fn()} />);
    expect(screen.getByText("Appearance")).toBeInTheDocument();
    expect(screen.getByText("Open at login")).toBeInTheDocument();
    expect(screen.queryByText("Updates")).not.toBeInTheDocument();
    expect(screen.queryByText("Menu bar icon")).not.toBeInTheDocument();
    screen.getByRole("button", { name: "Dark" }).focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(updateSettings).toHaveBeenCalledWith({ theme: "dark" }));
    fireEvent.click(screen.getByRole("switch", { name: "Open at login" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_autostart", { enabled: true }));
    view.rerender(<SettingsTab pane="about" onPaneChange={vi.fn()} />);
    expect(await screen.findByText("Update channel")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("switch", { name: "Updates" }));
    await waitFor(() =>
      expect(updateSettings).toHaveBeenCalledWith({ check_updates_automatically: true }),
    );
    expect(await screen.findByText("Voicetypr 2.1.0")).toBeInTheDocument();
  });

  it("renders shortcut actions and opens Recording for the primary shortcut", async () => {
    const onNavigate = vi.fn();
    render(<SettingsTab pane="shortcuts" onPaneChange={vi.fn()} onNavigate={onNavigate} />);
    expect(await screen.findByText("Cancel dictation")).toBeInTheDocument();
    expect(screen.getByText("Copy last transcript")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Manage in Recording" }));
    expect(onNavigate).toHaveBeenCalledWith("recording");
  });

  it("renders Privacy's real switches and persists analytics consent", async () => {
    render(<SettingsTab pane="privacy" onPaneChange={vi.fn()} />);
    const analytics = await screen.findByRole("switch", { name: "Enable usage analytics" });
    expect(
      screen.getByRole("switch", { name: "Enable crash and error reporting" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Audio, transcripts, clipboard contents/)).toBeInTheDocument();
    fireEvent.click(analytics);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_product_analytics_consent", { enabled: false }),
    );
  });

  it("renders Storage's existing retention and folder actions", async () => {
    const onNavigate = vi.fn();
    render(<SettingsTab pane="storage" onPaneChange={vi.fn()} onNavigate={onNavigate} />);
    expect(screen.getByText("Keep history")).toBeInTheDocument();
    expect(screen.getByText("Keep audio recordings")).toBeInTheDocument();
    expect(await screen.findByText("/Users/demo/Recordings")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open folder" }));
    expect(invoke).toHaveBeenCalledWith("open_recordings_folder");
    await userEvent.click(screen.getByRole("button", { name: "Manage in Transcription" }));
    expect(onNavigate).toHaveBeenCalledWith("transcription");
  });

  it("renders Network sharing's address, password, firewall and device status", async () => {
    render(<SettingsTab pane="network" onPaneChange={vi.fn()} />);
    expect(
      await screen.findByRole("button", { name: "Copy address 192.168.1.20:4894" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    expect(screen.getByLabelText("Password (Optional)")).toBeInTheDocument();
    expect(screen.getByText("Firewall")).toBeInTheDocument();
    expect(screen.getByText("Connected devices")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Copy address 192.168.1.20:4894" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalled();
  });

  it("keeps Network sharing's password and toggle command flows", async () => {
    render(<SettingsTab pane="network" onPaneChange={vi.fn()} />);
    const toggle = await screen.findByRole("switch", { name: "Share this Voicetypr" });
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    fireEvent.change(screen.getByLabelText("Password (Optional)"), {
      target: { value: "new-passphrase" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "start_sharing",
        expect.objectContaining({ password: "new-passphrase" }),
      ),
    );
    await userEvent.click(toggle);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("stop_sharing"));
  });

  it("discards a cancelled password before sharing is toggled off and on", async () => {
    const fixtures = createFixtures({ platform: "macos", theme: "light", empty: false });
    let sharingEnabled = true;
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "stop_sharing") sharingEnabled = false;
      if (command === "start_sharing") sharingEnabled = true;
      if (command === "get_sharing_status")
        return Promise.resolve({
          ...(fixtures.get_sharing_status as Record<string, unknown>),
          enabled: sharingEnabled,
        });
      return Promise.resolve(fixtures[command]);
    });
    render(<SettingsTab pane="network" onPaneChange={vi.fn()} />);
    const toggle = await screen.findByRole("switch", { name: "Share this Voicetypr" });
    await userEvent.click(screen.getByRole("button", { name: "Change" }));
    fireEvent.change(screen.getByLabelText("Password (Optional)"), {
      target: { value: "cancelled-secret" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await userEvent.click(toggle);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("stop_sharing"));
    await userEvent.click(toggle);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("start_sharing", expect.anything()));
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_sharing"),
    ).not.toEqual(
      expect.arrayContaining([
        ["start_sharing", expect.objectContaining({ password: "cancelled-secret" })],
      ]),
    );
  });

  it("gives every switch in each Settings pane an accessible name", async () => {
    for (const pane of [
      "general",
      "shortcuts",
      "privacy",
      "storage",
      "network",
      "agent",
      "advanced",
    ] as const) {
      const view = render(<SettingsTab pane={pane} onPaneChange={vi.fn()} />);
      if (pane === "network") await screen.findByRole("switch", { name: "Share this Voicetypr" });
      for (const control of screen.queryAllByRole("switch")) {
        expect(control).toHaveAccessibleName();
      }
      view.unmount();
    }
  });

  it("renders CLI status and repair from the existing commands", async () => {
    render(<SettingsTab pane="agent" onPaneChange={vi.fn()} />);
    expect(await screen.findByText("Installed")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Repair" }));
    expect(invoke).toHaveBeenCalledWith("repair_cli_tool");
    expect(screen.getByRole("button", { name: "Copy agent prompt" })).toBeInTheDocument();
  });

  it("shows the Windows microphone privacy row and reset confirmation", async () => {
    platform.isMacOS = false;
    platform.isWindows = true;
    render(<SettingsTab pane="advanced" onPaneChange={vi.fn()} />);
    expect(
      screen.getByText("Windows Settings → Privacy → Microphone must allow desktop apps."),
    ).toBeInTheDocument();
    expect(screen.queryByText("Accessibility")).not.toBeInTheDocument();
    expect(screen.queryByText("Input monitoring")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Reset…" }));
    expect(ask).toHaveBeenCalledWith(
      expect.stringContaining("except your license and API keys"),
      expect.objectContaining({ title: "Reset App Data" }),
    );
  });
});
