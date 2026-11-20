import type { DiscoveryArtKind } from "@/components/discovery/DiscoveryArt";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import { isMacOS } from "@/lib/platform";

export type CapabilityBadge = "Works now" | "Needs a provider" | "Needs a key" | "Needs a second computer" | "Download" | "Needs a source that streams" | "Turn on first" | "Install first";
export type CapabilityTarget = { screen: ScreenId; label: string } | { pane: SettingsPane; label: string };

export interface Capability {
  id: string;
  art: DiscoveryArtKind;
  title: string;
  line: string;
  badge: CapabilityBadge;
  target?: CapabilityTarget;
}

const computer = isMacOS ? "Mac" : "PC";

/** Everything Voicetypr can do, grouped as in the MOI-154 design. Copy follows the design's truthfulness review. */
export function capabilityGroups(streams: boolean, polishReady: boolean): { title: string; items: Capability[] }[] {
  return [
    {
      title: "Write better",
      items: [
        { id: "polish", art: "polish", title: "Polish", line: "Cleans up fillers, false starts and punctuation, with a style per app.", badge: polishReady ? "Works now" : "Needs a provider", target: { screen: "polish", label: "Open Polish" } },
        { id: "dictionary", art: "dictionary", title: "Dictionary", line: "Add names and terms so Voicetypr gets them right.", badge: "Works now", target: { screen: "dictionary", label: "Open Dictionary" } },
        { id: "suggestions", art: "suggestion", title: "Dictionary suggestions", line: "Fix a word in History and the Dictionary offers to learn it.", badge: "Works now", target: { screen: "history", label: "Open History" } },
        { id: "voice_commands", art: "voice_commands", title: "Voice commands and spoken punctuation", line: `Say only the command, like “new line” or “semicolon”, then stop${isMacOS ? "; needs Accessibility" : ""}.`, badge: "Turn on first", target: { screen: "polish", label: "Open Polish" } },
      ],
    },
    {
      title: "Using it",
      items: [
        { id: "hold", art: "hold", title: "Shortcut and Hold to talk", line: "Press your shortcut to start and stop, or hold it while you talk.", badge: "Works now", target: { screen: "recording", label: "Open Recording" } },
        { id: "pill", art: "pill", title: "The pill", line: "Hover it for Stop and Cancel; its chips switch style, engine, microphone and language.", badge: "Works now", target: { screen: "recording", label: "Open Recording" } },
        streams
          ? { id: "realtime", art: "realtime", title: "Real-time words", line: "See the text while you talk.", badge: "Works now", target: { screen: "transcription", label: "Open Transcription" } }
          : { id: "realtime", art: "realtime", title: "Real-time words", line: "See the text while you talk.", badge: "Needs a source that streams", target: { screen: "transcription", label: "Open Transcription" } },
        { id: "kept", art: "retry", title: "Retry a failed take", line: "If a take can't be transcribed, retry it from Home while its audio is kept.", badge: "Works now", target: { pane: "storage", label: "Open Storage" } },
        { id: "tray", art: "tray", title: isMacOS ? "Menu bar" : "System tray", line: `Start, stop, retry and switch settings from the Voicetypr icon in the ${isMacOS ? "menu bar" : "system tray"}.`, badge: "Works now" },
        { id: "media", art: "media", title: "Pause media and sounds", line: "Pause playing media while you dictate, and choose optional sounds.", badge: "Works now", target: { screen: "recording", label: "Open Recording" } },
      ],
    },
    {
      title: "Engines",
      items: [
        { id: "on_device", art: "model", title: "On-device models", line: "Private and offline, from about 150 MB to 3.1 GB.", badge: "Download", target: { screen: "transcription", label: "Open Transcription" } },
        { id: "cloud", art: "cloud", title: "Cloud, with your key", line: "Soniox, Deepgram, OpenAI, Groq or Cohere; your audio goes to that provider.", badge: "Needs a key", target: { screen: "transcription", label: "Open Transcription" } },
        { id: "remote", art: "remote", title: "Another computer", line: "Send your audio to a stronger Voicetypr on your network.", badge: "Needs a second computer", target: { screen: "transcription", label: "Open Transcription" } },
        { id: "share", art: "share", title: `Share this ${computer}`, line: "Let your other computers on a trusted network use this one as their engine.", badge: "Needs a second computer", target: { pane: "network", label: "Open Network sharing" } },
      ],
    },
    {
      title: "Your data",
      items: [
        { id: "history", art: "history", title: "History and export", line: `Transcripts stay on this ${computer}, ready to search, edit, copy or export.`, badge: "Works now", target: { screen: "history", label: "Open History" } },
        { id: "file", art: "file", title: "Transcribe a file", line: "Drop in a recording or a video and get the text from your current source.", badge: "Works now", target: { screen: "audio", label: "Choose a file" } },
        { id: "insights", art: "insights", title: "Insights and the share card", line: "Words, speed and time saved, with a card to share.", badge: "Works now", target: { screen: "insights", label: "Open Insights" } },
      ],
    },
    {
      title: "For developers",
      items: [
        { id: "cli", art: "terminal", title: "Command line and agent prompt", line: "Transcribe files from your terminal, or hand the voicetypr command to your AI agent.", badge: "Install first", target: { pane: "agent", label: "Open CLI & API" } },
      ],
    },
  ];
}
