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
  await user.click(screen.getByRole("tab", { name: new RegExp(`^${name} \\d+$`) }));
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
    await user.click(screen.getByRole("button", { name: "Save" }));
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
    await user.click(screen.getByRole("button", { name: "Save" }));
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
    await user.click(screen.getByRole("button", { name: "Save" }));
    await user.type(screen.getByRole("textbox", { name: "Search corrections" }), "other");
    expect(region.queryByRole("cell", { name: "voice typer" })).not.toBeInTheDocument();
    await user.clear(screen.getByRole("textbox", { name: "Search corrections" }));
    expect(region.getByRole("cell", { name: "Voicetypr" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Edit row 1" }));
    await user.clear(screen.getByLabelText("Replace"));
    await user.type(screen.getByLabelText("Replace"), "VoiceTypr");
    await user.click(screen.getByRole("button", { name: "Save" }));
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
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(useWritingSettings.getState().settings.snippets[0]).toMatchObject({ trigger: "insert signature", body: "Best, Moinul", preserve_literal: true });
    await user.type(screen.getByRole("textbox", { name: "Search snippets" }), "absent");
    expect(region.queryByRole("cell", { name: "insert signature" })).not.toBeInTheDocument();
    await user.clear(screen.getByRole("textbox", { name: "Search snippets" }));
    await user.click(screen.getByRole("button", { name: "Edit row 1" }));
    await user.clear(screen.getByLabelText("Text to insert"));
    await user.type(screen.getByLabelText("Text to insert"), "Thanks, Moinul");
    await user.click(screen.getByRole("switch", { name: "Keep text exact" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
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
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await act(async () => { releaseFirst?.(); });
    await waitFor(() => {
      const calls = vi.mocked(invoke).mock.calls.filter(([command]) => command === "update_writing_settings");
      expect(calls).toHaveLength(3);
      expect(calls[2]?.[1]).toEqual({ settings: expect.objectContaining({
        app_formatting_rules: [expect.objectContaining({ app_name: "Slack" })],
        custom_words: [expect.objectContaining({ phrase: "Voicetypr" })],
      }) });
    });
  });
});

describe("Dictionary regression cases", () => {
  it.each([
    ["words", "custom_words", { phrase: "Alpha", enabled: true }, { phrase: "Beta", enabled: true }, "Canonical"],
    ["corrections", "replacements", { from: "Alpha", to: "A", enabled: true }, { from: "Beta", to: "B", enabled: true }, "Match"],
    ["snippets", "snippets", { trigger: "insert Alpha", body: "A", enabled: true, preserve_literal: true }, { trigger: "insert Beta", body: "B", enabled: true, preserve_literal: true }, "Insert"],
  ] as const)("keeps %s editing attached to Beta across a rejected delete", async (kind, key, alpha, beta, field) => {
    const initial = { ...defaultWritingSettings, [key]: [alpha, beta] };
    let saves = 0;
    let rejectSave: ((reason: Error) => void) | undefined;
    const pending = new Promise<void>((_, reject) => { rejectSave = reject; });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_writing_settings") return initial;
      if (command === "update_writing_settings") return ++saves === 1 ? pending : undefined;
      return undefined;
    });
    const user = userEvent.setup();
    render(<DictionarySection />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Add word" })).toBeEnabled());
    if (kind !== "words") await user.click(screen.getByRole("tab", { name: new RegExp(`^${kind}`, "i") }));
    await user.click(screen.getByRole("button", { name: "Delete row 1" }));
    await user.click(screen.getByRole("button", { name: "Edit row 1" }));
    await act(async () => { rejectSave?.(new Error("save rejected")); });
    if (screen.queryByLabelText(field)) {
      await user.clear(screen.getByLabelText(field));
      await user.type(screen.getByLabelText(field), kind === "snippets" ? "Gamma" : "Gamma");
      await user.click(screen.getByRole("button", { name: "Save" }));
      const rows = useWritingSettings.getState().settings[key];
      expect(rows[0]).toEqual(alpha);
      expect(JSON.stringify(rows[1])).toContain("Gamma");
    } else {
      expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    }
  });

  it("uses tablist, tabs, panels and arrow keys", async () => {
    const user = userEvent.setup();
    await ready();
    const tabs = screen.getByRole("tablist", { name: "Dictionary tabs" });
    expect(within(tabs).getAllByRole("tab")).toHaveLength(3);
    const words = within(tabs).getByRole("tab", { name: /^Words/ });
    expect(words).toHaveAttribute("aria-controls");
    expect(screen.getByRole("tabpanel")).toHaveAttribute("id", words.getAttribute("aria-controls"));
    words.focus();
    await user.keyboard("{ArrowRight}");
    expect(within(tabs).getByRole("tab", { name: /^Corrections/ })).toHaveAttribute("aria-selected", "true");
  });

  it("closes a word editor when an external list reorder moves its source", async () => {
    const alpha = { phrase: "Alpha", enabled: true };
    const beta = { phrase: "Beta", enabled: true };
    vi.mocked(invoke).mockImplementation(async (command) => command === "get_writing_settings"
      ? { ...defaultWritingSettings, custom_words: [alpha, beta] } : undefined);
    const user = userEvent.setup();
    render(<DictionarySection />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Add word" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "Edit row 2" }));
    expect(screen.getByLabelText("Canonical")).toHaveValue("Beta");
    act(() => useWritingSettings.setState((state) => ({ settings: { ...state.settings, custom_words: [beta, alpha] } })));
    expect(screen.queryByLabelText("Canonical")).not.toBeInTheDocument();
  });

  it.each([
    ["words", "custom_words", { phrase: " Alpha ", enabled: true }, "Canonical", "alpha", /already exists/],
    ["corrections", "replacements", { from: " Alpha ", to: "A", enabled: true }, "Match", "alpha", /already exists/],
    ["snippets", "snippets", { trigger: "insert Alpha", body: "A", language: null, enabled: true, preserve_literal: true }, "Insert", "ALPHA", /already exists/],
  ] as const)("rejects normalized duplicate %s entries", async (kind, key, existing, field, value, message) => {
    vi.mocked(invoke).mockImplementation(async (command) => command === "get_writing_settings" ? { ...defaultWritingSettings, [key]: [existing] } : undefined);
    const user = userEvent.setup();
    render(<DictionarySection />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Add word" })).toBeEnabled());
    if (kind !== "words") await user.click(screen.getByRole("tab", { name: new RegExp(`^${kind}`, "i") }));
    await user.click(screen.getByRole("button", { name: kind === "words" ? "Add word" : kind === "corrections" ? "Add rule" : "Add saved text" }));
    await user.type(screen.getByLabelText(field), value);
    if (kind === "corrections") await user.type(screen.getByLabelText("Replace"), "B");
    if (kind === "snippets") await user.type(screen.getByLabelText("Text to insert"), "B");
    expect(screen.getByRole("alert")).toHaveTextContent(message);
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("rejects whitespace-only words and shows the inline error", async () => {
    const user = userEvent.setup();
    await ready();
    await user.click(screen.getByRole("button", { name: "Add word" }));
    await user.type(screen.getByLabelText("Canonical"), "   ");
    expect(screen.getByRole("alert")).toHaveTextContent("Enter a word or phrase.");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("blocks snippet triggers in overlapping scopes and allows distinct languages", async () => {
    const existing = { trigger: "insert note", body: "A", language: "en", enabled: true, preserve_literal: true };
    vi.mocked(invoke).mockImplementation(async (command) => command === "get_writing_settings"
      ? { ...defaultWritingSettings, snippets: [existing] } : undefined);
    const user = userEvent.setup();
    render(<DictionarySection />);
    await waitFor(() => expect(screen.getByRole("button", { name: "Add word" })).toBeEnabled());
    await user.click(screen.getByRole("tab", { name: /^Snippets/ }));
    await user.click(screen.getByRole("button", { name: "Add saved text" }));
    await user.type(screen.getByLabelText("Insert"), "NOTE");
    await user.type(screen.getByLabelText("Text to insert"), "B");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    await user.type(screen.getByLabelText("Language"), "de");
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
    await user.clear(screen.getByLabelText("Language"));
    await user.type(screen.getByLabelText("Language"), "en");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows the Off label for an existing per-app override", () => {
    render(<EnhancementSettings preset="CleanDictation" finalTextLanguage="same_as_transcript"
      writingSettings={{ ...defaultWritingSettings, app_formatting_rules: [{ app_name: "Slack", preset: "PersonalDictation", enabled: true }] }}
      aiFormattingEnabled={false} providerContent={<p>Provider</p>} onPresetChange={() => {}}
      onFinalTextLanguageChange={() => {}} onWritingSettingsChange={() => {}} />);
    expect(screen.getByRole("combobox", { name: "Style for Slack" })).toHaveTextContent("Off");
    expect(screen.getByRole("combobox", { name: "Style for Slack" })).not.toHaveTextContent("PersonalDictation");
  });
});
