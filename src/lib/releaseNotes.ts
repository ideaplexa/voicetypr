import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";

export function releasePageUrl(version: string): string {
  return `https://github.com/ideaplexa/voicetypr/releases/tag/v${encodeURIComponent(version.replace(/^v/, ""))}`;
}

/** Release links use the existing opener plugin, never the app's webview. */
export async function openReleaseLink(url: string): Promise<void> {
  try {
    await invoke("plugin:opener|open_url", { url, with: null });
  } catch {
    toast.error("Couldn't open the release page. Please try again.");
  }
}

export function releaseHighlights(body: string | null | undefined): string | null {
  const text = body?.trim();
  if (!text || /^see (?:the )?(?:full )?release notes\b/i.test(text)) return null;
  return text;
}
