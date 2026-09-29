import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RecordingSettings } from "../RecordingSettings";
import type { AppSettings } from "@/types";
import type { ShortcutBinding } from "@/types/shortcuts";

const refreshSettings = vi.fn().mockResolvedValue(undefined);
let settings: AppSettings;
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings, refreshSettings, updateSettings: vi.fn() }) }));
vi.mock("@/contexts/ReadinessContext", () => ({ useCanAutoInsert: () => true }));
vi.mock("@/lib/platform", () => ({ isMacOS: true, isWindows: false }));
const input = vi.hoisted(() => ({ onChange: null as null | ((value: string) => void) }));
vi.mock("@/components/HotkeyInput", () => ({ HotkeyInput: ({ onChange }: { onChange?: (value: string) => void }) => { input.onChange = onChange ?? null; return <div />; } }));
const invoke = vi.fn<(command: string, args?: { request?: { kind: string; value: string; mode: string } }) => Promise<unknown>>();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (command: string, args?: { request?: { kind: string; value: string; mode: string } }) => invoke(command, args) }));
vi.mock("@tauri-apps/plugin-autostart", () => ({ enable: vi.fn(), disable: vi.fn(), isEnabled: vi.fn() }));
vi.mock("@/components/MicrophoneSelection", () => ({ MicrophoneSelection: () => <div /> }));
vi.mock("@/components/sections/NetworkSharingCard", () => ({ NetworkSharingCard: () => <div /> }));
vi.mock("@/components/ui/scroll-area", () => ({ ScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div> }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

const binding = (id: string, enabled = true, action: ShortcutBinding["action"] = "hold_to_record"): ShortcutBinding => ({
  id, enabled, action, shortcut: "", trigger: action === "hold_to_record" ? "hold" : "pressed",
  allow_risky_combo: false, trigger_kind: "modifier_hold", modifier: { modifier: "meta", side: "right" },
});
const base = { hotkey: "", recording_mode: "toggle", current_model: "", speech_language: "en", theme: "system" } as AppSettings;
let bindings: ShortcutBinding[];
let hotkey: string;
const effective = () => {
  const primary = hotkey ? null : bindings.find((item) => item.enabled && (item.action === "hold_to_record" || item.action === "toggle_recording")) ?? null;
  return { binding: primary, hotkey: hotkey || (primary ? null : "CommandOrControl+Shift+Space"), mode: primary?.action === "hold_to_record" ? "hold" : "toggle" };
};

beforeEach(() => {
  settings = { ...base };
  hotkey = "";
  bindings = [binding("primary-A"), binding("additional-B", true, "toggle_recording"), binding("cancel", true, "cancel_recording")];
  vi.clearAllMocks();
  invoke.mockImplementation(async (command, args) => {
    if (command === "get_effective_primary_shortcut") return effective();
    if (command === "set_primary_recording_shortcut") {
      const captured = effective();
      if (captured.binding) bindings = bindings.map((item) => item.id === captured.binding?.id ? { ...item, enabled: false } : item);
      hotkey = args?.request?.kind === "combo" ? args.request.value : "";
      return { binding: null, hotkey, mode: args?.request?.mode };
    }
    if (command === "get_autostart_status") return false;
    return undefined;
  });
});

async function saveCombo() {
  render(<RecordingSettings />);
  await screen.findByLabelText("Current shortcut: Hold Right ⌘ to talk");
  fireEvent.click(screen.getByRole("button", { name: "Change" }));
  act(() => input.onChange?.("CommandOrControl+Shift+Space"));
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_primary_recording_shortcut", { request: { kind: "combo", value: "CommandOrControl+Shift+Space", mode: "hold" } }));
}

describe("atomic primary replacement", () => {
  it("captures native Hold before the Rust state mutation despite stale toggle", async () => {
    await saveCombo();
    expect(bindings.find((item) => item.id === "primary-A")?.enabled).toBe(false);
    expect(settings.recording_mode).toBe("toggle");
    expect(invoke).not.toHaveBeenCalledWith("set_global_shortcut", expect.anything());
  });
  it("disables only the captured primary and leaves additional and cancel bindings", async () => {
    await saveCombo();
    expect(bindings.find((item) => item.id === "additional-B")?.enabled).toBe(true);
    expect(bindings.find((item) => item.id === "cancel")?.enabled).toBe(true);
    expect(invoke.mock.calls.filter(([command]) => command === "set_primary_recording_shortcut")).toHaveLength(1);
  });
  it("uses the active binding when the preferred id is disabled", async () => {
    bindings = [binding("onboarding-primary-hold", false), binding("custom-hold"), binding("cancel", true, "cancel_recording")];
    await saveCombo();
    expect(bindings.find((item) => item.id === "custom-hold")?.enabled).toBe(false);
  });
});
