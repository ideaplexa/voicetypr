import { AgentCliSection } from "../sections/AgentCliSection";
import { SettingsPage, SettingsHeader } from "@/components/settings/settings-ui";

export function AgentCliTab() {
  return (
    <SettingsPage>
      <SettingsHeader
        title="CLI"
        description="Transcribe from the terminal and connect Voicetypr to scripts or AI agents."
      />
      <AgentCliSection />
    </SettingsPage>
  );
}
