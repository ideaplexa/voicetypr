import { createUsageFixture } from "@/ui-preview/usageFixture";
import { installOnboardingPreview } from "@/components/onboarding/onboardingPreview";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { createFixtures, demoDownloadProgress, type PreviewOptions } from "./fixtures";
import { installPreviewPlatform } from "./platform";

const query = new URLSearchParams(location.search);
const onboarding = query.get("onboarding");
const onboardingPhase =
  onboarding === "1" ? 1 : onboarding === "2" ? 2 : onboarding === "3" ? 3 : undefined;
if (onboardingPhase) installOnboardingPreview(onboardingPhase);
const options: PreviewOptions = {
  theme: query.get("theme") === "dark" ? "dark" : "light",
  platform: query.get("platform") === "windows" ? "windows" : "macos",
  empty: query.get("empty") === "1",
  sidebar: query.get("sidebar") === "rail" ? "rail" : "expanded",
  onboarding: onboardingPhase,
};
document.cookie = `sidebar_state=${options.sidebar !== "rail"}; path=/`;
installPreviewPlatform(options.platform);
const fixtures = createFixtures(options);
const unknown = new Set<string>();
mockWindows("main");
mockIPC(
  (command, args) => {
    if (command === "get_usage_stats") {
      const since = args && typeof args === "object" && "since" in args && typeof args.since === "string" ? args.since : null;
      return createUsageFixture(since, options.empty);
    }
    if (command.startsWith("plugin:event|")) return null;
    if (Object.prototype.hasOwnProperty.call(fixtures, command)) return fixtures[command];
    if (!unknown.has(command)) {
      unknown.add(command);
      console.warn(`[ui-preview] unknown command: ${command}`);
    }
    return null;
  },
  { shouldMockEvents: true },
);

// Platform and IPC must exist before modules with module-level Tauri reads load.
await import("@/main");

// Let the app register its listeners before seeding a visible in-progress model.
window.setTimeout(() => {
  void emit("download-progress", demoDownloadProgress);
}, 1200);

// Direct modal fixtures use the same app event path as tray/deep-link navigation.
const shot = query.get("screen");
if (shot === "license" || shot?.startsWith("settings-")) {
  window.setTimeout(() => {
    void emit(
      "navigate-to-settings",
      shot === "license" ? "license" : shot.slice("settings-".length),
    );
  }, 1200);
}
