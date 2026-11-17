import { createUsageFixture } from "@/ui-preview/usageFixture";
import { installOnboardingPreview } from "@/components/onboarding/onboardingPreview";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { createFixtures, demoDownloadProgress, type OnboardingPreviewCase, type PreviewOptions } from "./fixtures";
import type { Step } from "@/components/onboarding/onboardingTypes";
import { installPreviewPlatform } from "./platform";

const query = new URLSearchParams(location.search);
const STEPS: Step[] = ["promise", "try", "source", "readiness", "setup", "trial", "done"];
// The numbered phases of the previous onboarding still open its nearest step.
const LEGACY: Record<string, Step> = { "1": "promise", "2": "setup", "3": "trial" };
const onboardingParam = query.get("onboarding") ?? "";
const onboardingStep = LEGACY[onboardingParam] ?? STEPS.find((step) => step === onboardingParam);
if (onboardingStep) installOnboardingPreview(onboardingStep);
const onboardingCase = (query.get("case") || undefined) as OnboardingPreviewCase | undefined;
const options: PreviewOptions = {
  theme: query.get("theme") === "dark" ? "dark" : "light",
  platform: query.get("platform") === "windows" ? "windows" : "macos",
  empty: query.get("empty") === "1",
  sidebar: query.get("sidebar") === "rail" ? "rail" : "expanded",
  onboarding: onboardingStep,
  onboardingCase,
  trialNotStarted: query.get("license") === "none",
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
    if (command === "start_trial" && onboardingCase === "offline") throw "trial_unreachable";
    if (command === "activate_license" && onboardingCase === "key-error") throw "Invalid license key";
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

// A finished first dictation, as the backend reports it after saving history.
if (onboardingCase === "heard" || onboardingCase === "polished") {
  // Repeat until the try step has subscribed; each emit shows the same result.
  let emits = 0;
  const timer = window.setInterval(() => {
    if (++emits === 6) window.clearInterval(timer);
    void emit("transcription-added", onboardingCase === "polished"
      ? {
          timestamp: "2026-11-17T09:41:00Z",
          text: "Let’s move the sync to Friday at 3, and I’ll send the notes before.",
          model: "parakeet-v3",
          writing: { ai_applied: true, original_text: "um so let's move the sync to thursday no wait friday at three and I'll send the notes before" },
        }
      : {
          timestamp: "2026-11-17T09:41:00Z",
          text: "Um, so let’s move the sync to Thursday, no wait, Friday at three, and I’ll send the notes before.",
          model: "parakeet-v3",
          writing: { ai_applied: false },
        });
  }, 1000);
}
