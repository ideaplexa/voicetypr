import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { createFixtures } from "./fixtures";
import { installPreviewPlatform } from "./platform";

vi.mock("@tauri-apps/plugin-os", () => ({
  type: () => window.__TAURI_OS_PLUGIN_INTERNALS__.os_type,
}));

afterEach(() => {
  installPreviewPlatform("macos");
  vi.resetModules();
});

describe("UI preview platform", () => {
  it.each(["macos", "windows"] as const)("uses %s copy and shortcuts", async (platform) => {
    installPreviewPlatform(platform);
    vi.resetModules();
    const { isMacOS, isWindows } = await import("@/lib/platform");
    const { formatHotkey } = await import("@/lib/hotkey-utils");
    const { RecentRecordingsHeader } = await import("@/components/sections/RecentRecordingsHeader");
    const osPlugin = await vi.importActual<typeof import("@tauri-apps/plugin-os")>(
      "@tauri-apps/plugin-os",
    );
    const os = window.__TAURI_OS_PLUGIN_INTERNALS__;
    const fixtures = createFixtures({ platform, theme: "light", empty: false });

    expect(os.os_type).toBe(platform);
    expect(os.platform).toBe(platform);
    expect(os.version).toBe(platform === "macos" ? "14.0" : "10.0");
    expect(os.family).toBe(platform === "macos" ? "unix" : "windows");
    expect(os.arch).toBe("x86_64");
    expect(osPlugin.type()).toBe(platform);
    expect(osPlugin.platform()).toBe(platform);
    expect(osPlugin.version()).toBe(os.version);
    expect(osPlugin.family()).toBe(os.family);
    expect(osPlugin.arch()).toBe(os.arch);
    expect(isMacOS).toBe(platform === "macos");
    expect(isWindows).toBe(platform === "windows");
    expect(fixtures.validate_microphone_selection).toBe(false);
    expect(fixtures.get_audio_devices).toContain(
      (fixtures.get_settings as { selected_microphone: string }).selected_microphone,
    );

    const shortcut = (fixtures.get_settings as { hotkey: string }).hotkey;
    const keys = renderToStaticMarkup(<>{formatHotkey(shortcut)}</>);
    expect(keys).toContain(platform === "macos" ? "⌥" : "Ctrl");
    expect(keys).toContain("Space");
    if (platform === "windows") expect(keys).toContain("Alt");

    const history = renderToStaticMarkup(
      <RecentRecordingsHeader
        historyLength={2}
        onExport={vi.fn()}
        onExportText={vi.fn()}
        onClearAll={vi.fn()}
      />,
    );
    expect(history).toContain(`stored on this ${platform === "macos" ? "Mac" : "PC"}`);
  });
});
