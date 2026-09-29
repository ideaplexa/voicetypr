import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { TabContainer } from "./TabContainer";
import { useState } from "react";
import type { SettingsPane } from "@/components/navigation";
import type { ScreenId } from "@/components/navigation";

vi.mock("./RecordingsTab", () => ({
  RecordingsTab: ({ onTranscribeFile }: { onTranscribeFile?: () => void }) => (
    <>
      <h1>History screen</h1>
      <button onClick={onTranscribeFile}>Transcribe a file…</button>
    </>
  ),
}));
vi.mock("./RecordingTab", () => ({ RecordingTab: () => <h1>Recording screen</h1> }));
vi.mock("./OverviewTab", () => ({ OverviewTab: () => <h1>Home screen</h1> }));
vi.mock("./ModelsTab", () => ({
  ModelsTab: ({ sourceFilter }: { sourceFilter?: string }) => (
    <h1>Transcription screen {sourceFilter}</h1>
  ),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("2.1.0") }));
vi.mock("@/components/sections/GeneralSettings", () => ({
  GeneralSettings: () => <p>General controls</p>,
}));
vi.mock("@/components/sections/ShortcutsSection", () => ({
  ShortcutsSection: () => <p>Shortcut controls</p>,
}));
vi.mock("@/components/sections/AdvancedSection", () => ({
  AdvancedSection: () => <p>Troubleshooting controls</p>,
}));
vi.mock("@/components/sections/NetworkSharingCard", () => ({
  NetworkSharingCard: () => <p>Network controls</p>,
}));
vi.mock("@/components/sections/AgentCliSection", () => ({
  AgentCliSection: () => <p>CLI controls</p>,
}));
vi.mock("./EnhancementsTab", () => ({ EnhancementsTab: () => <h1>Polish screen</h1> }));
vi.mock("@/components/sections/DictionarySection", () => ({
  DictionarySection: () => <h1>Dictionary screen</h1>,
}));
vi.mock("./AccountTab", () => ({ AccountTab: () => <h1>License screen</h1> }));
vi.mock("../sections/ReportProblemSection", () => ({
  ReportProblemSection: ({
    onNavigateSettingsPane,
  }: {
    onNavigateSettingsPane: (pane: SettingsPane) => void;
  }) => (
    <>
      <h1>Help screen</h1>
      <button onClick={() => onNavigateSettingsPane("advanced")}>Troubleshooting</button>
      <button onClick={() => onNavigateSettingsPane("shortcuts")}>Shortcuts</button>
    </>
  ),
}));
vi.mock("../sections/AudioUploadSection", () => ({
  AudioUploadSection: () => <p>Choose audio file</p>,
}));

describe("TabContainer destinations", () => {
  it.each<[ScreenId, string]>([
    ["home", "Home screen"],
    ["history", "History screen"],
    ["transcription", "Transcription screen"],
    ["polish", "Polish screen"],
    ["dictionary", "Dictionary screen"],
    ["recording", "Recording screen"],
    ["settings", "Settings"],
    ["help", "Help screen"],
    ["license", "License screen"],
  ])("opens %s with its existing content", (id, heading) => {
    render(<TabContainer activeSection={id} />);
    expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
  });

  it.each<[ScreenId, string]>([
    ["overview", "Home screen"],
    ["recordings", "History screen"],
    ["models", "Transcription screen"],
    ["formatting", "Polish screen"],
    ["general", "Settings"],
    ["shortcuts", "Settings"],
    ["network", "Settings"],
    ["agent", "Settings"],
    ["advanced", "Settings"],
    ["report-problem", "Help screen"],
  ])("keeps the %s alias reachable", (id, heading) => {
    render(<TabContainer activeSection={id} />);
    expect(screen.getByRole("heading", { name: heading })).toBeInTheDocument();
  });

  it("passes Cloud filter to Transcription", () => {
    render(<TabContainer activeSection="transcription" sourceFilter="cloud" />);
    expect(screen.getByRole("heading", { name: "Transcription screen cloud" })).toBeInTheDocument();
  });

  it("applies an alias after an inner Settings pane click", async () => {
    const user = userEvent.setup();
    const view = render(<TabContainer activeSection="settings" />);
    await user.click(screen.getByRole("button", { name: "Shortcuts" }));
    expect(screen.getByText("Shortcut controls")).toBeInTheDocument();
    view.rerender(<TabContainer activeSection="general" />);
    expect(screen.getByRole("button", { name: "General" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByText("General controls")).toBeInTheDocument();
  });

  it("reports inner Settings pane clicks to navigation", async () => {
    const onSettingsPaneChange = vi.fn();
    render(
      <TabContainer
        activeSection="settings"
        settingsPane="general"
        onSettingsPaneChange={onSettingsPaneChange}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Shortcuts" }));
    expect(onSettingsPaneChange).toHaveBeenCalledWith("shortcuts");
  });

  it("opens file transcription from History and from the audio alias", async () => {
    const user = userEvent.setup();
    const view = render(<TabContainer activeSection="history" />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Transcribe a file…" }));
    expect(screen.getByRole("dialog", { name: "Transcribe a file…" })).toHaveTextContent(
      "Choose audio file",
    );
    expect(screen.getAllByRole("heading", { name: "Transcribe a file…" })).toHaveLength(1);
    view.rerender(<TabContainer activeSection="audio" />);
    expect(screen.getByRole("dialog", { name: "Transcribe a file…" })).toBeInTheDocument();
  });
  it.each([
    ["Troubleshooting", "Troubleshooting controls"],
    ["Shortcuts", "Shortcut controls"],
  ])("navigates Help's %s tile to its Settings pane", async (label, controls) => {
    function Shell() {
      const [activeSection, setActiveSection] = useState<ScreenId>("help");
      return <TabContainer activeSection={activeSection} onNavigate={setActiveSection} />;
    }
    render(<Shell />);
    await userEvent.click(screen.getByRole("button", { name: label }));
    expect(screen.getByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByText(controls)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: label })).toHaveAttribute("aria-current", "page");
  });
  it.each([
    ["Troubleshooting", "Troubleshooting controls"],
    ["Shortcuts", "Shortcut controls"],
  ] as const)(
    "keeps Help's %s destination with the app's controlled navigation",
    async (label, controls) => {
      const onNavigate = vi.fn();
      function Shell() {
        const [navigation, setNavigation] = useState<{
          activeSection: ScreenId;
          settingsPane?: SettingsPane;
        }>({ activeSection: "help" });
        return (
          <TabContainer
            {...navigation}
            onNavigate={(activeSection) => {
              onNavigate(activeSection);
              setNavigation({ activeSection, settingsPane: "general" });
            }}
            onSettingsPaneChange={(settingsPane) =>
              setNavigation({ activeSection: "settings", settingsPane })
            }
          />
        );
      }
      render(<Shell />);
      await userEvent.click(screen.getByRole("button", { name: label }));
      expect(screen.getByText(controls)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: label })).toHaveAttribute("aria-current", "page");
      expect(onNavigate).not.toHaveBeenCalled();
    },
  );
});
