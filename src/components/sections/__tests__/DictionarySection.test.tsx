import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { DictionarySection } from "../DictionarySection";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { useWritingSettings } from "@/state/writingSettings";
import { defaultWritingSettings } from "@/types/writing";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

beforeEach(() => {
  vi.clearAllMocks();
  useWritingSettings.setState({ settings: defaultWritingSettings, loaded: false });
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_writing_settings") return defaultWritingSettings;
    return undefined;
  });
});

async function ready() {
  render(<DictionarySection />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Add word" })).toBeEnabled());
}

async function selectTab(user: ReturnType<typeof userEvent.setup>, name: "Corrections" | "Snippets") {
  await user.click(screen.getByRole("button", { name: new RegExp(`^${name} \\d+$`) }));
  return within(screen.getByRole("region", { name }));
}

describe("Dictionary", () => {
  it("adds, edits, searches and deletes words with their spoken form and language", async () => {
    const user = userEvent.setup();
    await ready();
    expect(screen.getByText("No words yet — add names, brands and jargon so Voicetypr spells them right.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add word" }));
    await user.type(screen.getByLabelText("Canonical"), "Voicetypr");
    await user.type(screen.getByLabelText("Spoken"), "voice typer");
    await user.type(screen.getByLabelText("Language"), "en");
    await user.click(screen.getByRole("button", { name: "Done" }));
    expect(useWritingSettings.getState().settings.custom_words).toEqual([
      { phrase: "Voicetypr", spoken_form: "voice typer", language: "en", enabled: true },
    ]);
    expect(screen.getByRole("cell", { name: "voice typer" })).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "Search words" }), "missing");
    expect(screen.queryByRole("cell", { name: "Voicetypr" })).not.toBeInTheDocument();
    await user.clear(screen.getByRole("textbox", { name: "Search words" }));
    await user.type(screen.getByRole("textbox", { name: "Search words" }), "voice typer");
    expect(screen.getByRole("cell", { name: "Voicetypr" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Edit row 1" }));
    await user.clear(screen.getByLabelText("Canonical"));
    await user.type(screen.getByLabelText("Canonical"), "VoiceTypr");
    await user.click(screen.getByRole("button", { name: "Delete row 1" }));
    expect(useWritingSettings.getState().settings.custom_words).toEqual([]);
  });

  it("adds, edits, searches and deletes corrections", async () => {
    const user = userEvent.setup();
    await ready();
    const region = await selectTab(user, "Corrections");
    expect(region.getByText("No corrections yet — add a phrase and its exact replacement.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add rule" }));
    await user.type(screen.getByLabelText("Match"), "voice typer");
    await user.type(screen.getByLabelText("Replace"), "Voicetypr");
    await user.click(screen.getByRole("button", { name: "Done" }));
    await user.type(screen.getByRole("textbox", { name: "Search corrections" }), "other");
    expect(region.queryByRole("cell", { name: "voice typer" })).not.toBeInTheDocument();
    await user.clear(screen.getByRole("textbox", { name: "Search corrections" }));
    expect(region.getByRole("cell", { name: "Voicetypr" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Edit row 1" }));
    await user.clear(screen.getByLabelText("Replace"));
    await user.type(screen.getByLabelText("Replace"), "VoiceTypr");
    expect(useWritingSettings.getState().settings.replacements[0]?.to).toBe("VoiceTypr");
    await user.click(screen.getByRole("button", { name: "Delete row 1" }));
    expect(useWritingSettings.getState().settings.replacements).toEqual([]);
  });

  it("adds, edits, searches and deletes snippets while retaining literal mode", async () => {
    const user = userEvent.setup();
    await ready();
    const region = await selectTab(user, "Snippets");
    expect(region.getByText("No snippets yet — save text to insert with a spoken trigger.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add saved text" }));
    await user.type(screen.getByLabelText("Insert"), "signature");
    await user.type(screen.getByLabelText("Text to insert"), "Best, Moinul");
    expect(useWritingSettings.getState().settings.snippets[0]).toMatchObject({ trigger: "insert signature", body: "Best, Moinul", preserve_literal: true });
    await user.click(screen.getByRole("button", { name: "Done" }));
    await user.type(screen.getByRole("textbox", { name: "Search snippets" }), "absent");
    expect(region.queryByRole("cell", { name: "insert signature" })).not.toBeInTheDocument();
    await user.clear(screen.getByRole("textbox", { name: "Search snippets" }));
    await user.click(screen.getByRole("button", { name: "Edit row 1" }));
    await user.clear(screen.getByLabelText("Text to insert"));
    await user.type(screen.getByLabelText("Text to insert"), "Thanks, Moinul");
    await user.click(screen.getByRole("switch", { name: "Keep text exact" }));
    expect(useWritingSettings.getState().settings.snippets[0]).toMatchObject({ body: "Thanks, Moinul", preserve_literal: false });
    await user.click(screen.getByRole("button", { name: "Delete row 1" }));
    expect(useWritingSettings.getState().settings.snippets).toEqual([]);
  });

  it("queues rapid Polish app-style and Dictionary edits in order with both in the last save", async () => {
    let releaseFirst: (() => void) | undefined;
    const firstSave = new Promise<void>((resolve) => { releaseFirst = resolve; });
    let writes = 0;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_writing_settings") return defaultWritingSettings;
      if (command === "update_writing_settings") {
        writes += 1;
        if (writes === 1) await firstSave;
      }
      return undefined;
    });
    function PolishAndDictionary() {
      const settings = useWritingSettings((state) => state.settings);
      const update = useWritingSettings((state) => state.update);
      const loaded = useWritingSettings((state) => state.loaded);
      return <>
        <EnhancementSettings preset="PersonalDictation" finalTextLanguage="same_as_transcript" writingSettings={settings}
          aiFormattingEnabled={false} providerContent={<p>Provider</p>} onPresetChange={() => {}}
          onFinalTextLanguageChange={() => {}} onWritingSettingsChange={update} writingSettingsDisabled={!loaded} />
        <DictionarySection />
      </>;
    }
    render(<PolishAndDictionary />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Add override" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "Add override" }));
    fireEvent.change(screen.getByRole("textbox", { name: "App name 1" }), { target: { value: "Slack" } });
    await userEvent.click(screen.getByRole("button", { name: "Add word" }));
    fireEvent.change(screen.getByLabelText("Canonical"), { target: { value: "Voicetypr" } });
    await act(async () => { releaseFirst?.(); });
    await waitFor(() => {
      const calls = vi.mocked(invoke).mock.calls.filter(([command]) => command === "update_writing_settings");
      expect(calls).toHaveLength(4);
      expect(calls[3]?.[1]).toEqual({ settings: expect.objectContaining({
        app_formatting_rules: [expect.objectContaining({ app_name: "Slack" })],
        custom_words: [expect.objectContaining({ phrase: "Voicetypr" })],
      }) });
    });
  });
});
