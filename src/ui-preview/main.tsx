import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { createFixtures, type PreviewOptions } from "./fixtures";
import { installPreviewPlatform } from "./platform";

const query = new URLSearchParams(location.search);
const options: PreviewOptions = {
  theme: query.get("theme") === "dark" ? "dark" : "light",
  platform: query.get("platform") === "windows" ? "windows" : "macos",
  empty: query.get("empty") === "1",
};
installPreviewPlatform(options.platform);
const fixtures = createFixtures(options);
const unknown = new Set<string>();
mockWindows("main");
mockIPC((command) => {
  if (command.startsWith("plugin:event|")) return null;
  if (Object.prototype.hasOwnProperty.call(fixtures, command)) return fixtures[command];
  if (!unknown.has(command)) {
    unknown.add(command);
    console.warn(`[ui-preview] unknown command: ${command}`);
  }
  return null;
}, { shouldMockEvents: true });

// Platform and IPC must exist before modules with module-level Tauri reads load.
await import("@/main");
