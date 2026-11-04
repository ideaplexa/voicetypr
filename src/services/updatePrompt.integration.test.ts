import { expect, it, vi } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";
import { UpdateService } from "@/services/updateService";

vi.unmock("@tauri-apps/plugin-dialog");

it.each([
  ["2.1.0-beta.6", "View notes"],
  ["v2.1.0-beta.6", "View notes"],
  ["2.1.0-beta.6", "Cancel"],
  ["v2.1.0-beta.6", "Cancel"],
])(
  "uses the real dialog plugin's three-button IPC contract for %s returning %s",
  async (version, choice) => {
    const calls: Array<{ command: string; args: unknown }> = [];
    mockIPC((command, args) => {
      calls.push({ command, args });
      if (command === "get_distribution_info") return { channel: "direct", is_store_install: false };
      if (command === "check_for_app_update")
        return { version, body: "# Manifest body must never appear", channel: "beta" };
      if (command === "plugin:dialog|message") return choice;
      return undefined;
    });
    // @ts-expect-error accessing private static for test isolation
    UpdateService.instance = undefined;
    const service = UpdateService.getInstance();
    await service.checkForUpdatesManually();

    expect(calls.find(({ command }) => command === "plugin:dialog|message")?.args).toEqual({
      message: `Voicetypr ${version} is available.\n\nDownload and install it now? The app will restart when recording ends.`,
      title: "Update Available",
      kind: "info",
      buttons: { YesNoCancelCustom: ["Update", "View notes", "Cancel"] },
    });
    const opener = calls.find(({ command }) => command === "plugin:opener|open_url");
    if (choice === "View notes") {
      expect(opener?.args).toEqual({
        url: "https://github.com/ideaplexa/voicetypr/releases/tag/v2.1.0-beta.6",
        with: null,
      });
    } else {
      expect(opener).toBeUndefined();
    }
    expect(calls.some(({ command }) => command === "install_app_update")).toBe(false);
    expect(service.getStatus()).toMatchObject({ install: "idle", available: { version } });
  },
);
