import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { DictionarySection } from "../DictionarySection";
import { RecentRecordings } from "../RecentRecordings";
import { useDictionarySuggestions } from "@/state/dictionarySuggestions";
import { useWritingSettings } from "@/state/writingSettings";
import { defaultWritingSettings } from "@/types/writing";
import type { TranscriptionHistory } from "@/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(async () => true) }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/contexts/ReadinessContext", () => ({
  useCanRecord: () => true,
  useCanAutoInsert: () => true,
  useReadiness: () => ({
    canRecord: true,
    licenseValid: true,
    licenseStatus: "licensed",
    selectedModelAvailable: true,
    remoteSelected: false,
    remoteAvailable: null,
    hasMicrophonePermission: true,
  }),
}));
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ settings: { current_model: "small.en", current_model_engine: "whisper" } }),
}));

const voicetypr = { id: "s1", heard: "voice typer", word: "Voicetypr", status: "pending" as const };
const dictation: TranscriptionHistory = {
  id: "2026-11-16T09:00:00Z",
  text: "I pushed the fix to voice typer.",
  timestamp: new Date("2026-11-16T09:00:00Z"),
  model: "parakeet-tdt-0.6b-v2",
};
let pending: (typeof voicetypr)[] = [];

beforeEach(() => {
  vi.clearAllMocks();
  pending = [];
  useWritingSettings.setState({ settings: defaultWritingSettings, loaded: false });
  useDictionarySuggestions.setState({ suggestions: [] });
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_writing_settings") return defaultWritingSettings;
    if (command === "list_dictionary_suggestions") return pending;
    if (command === "update_transcription_text") return [voicetypr];
    return null;
  });
});

const undoLastToast = () => {
  const options = vi.mocked(toast.success).mock.lastCall?.[1] as
    | { action?: { onClick: () => void } }
    | undefined;
  options?.action?.onClick();
};

describe("Dictionary learns from corrections", () => {
  it("offers a word fixed in History and adds it with what was heard", async () => {
    const user = userEvent.setup();
    render(<RecentRecordings history={[dictation]} onHistoryUpdate={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "Edit text" }));
    const editor = screen.getByRole("textbox", { name: "Dictation text" });
    await user.clear(editor);
    await user.type(editor, "I pushed the fix to Voicetypr.");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(invoke).toHaveBeenCalledWith("update_transcription_text", {
      id: dictation.id,
      text: "I pushed the fix to Voicetypr.",
    });
    const offered = within(await screen.findByRole("region", { name: "Suggested words" }));
    expect(offered.getByText("heard as “voice typer”")).toBeInTheDocument();
    expect(screen.getByText("I pushed the fix to Voicetypr.")).toBeInTheDocument();

    await user.click(offered.getByRole("button", { name: "Add Voicetypr to Dictionary" }));
    await waitFor(() =>
      expect(useWritingSettings.getState().settings.custom_words).toEqual([
        { phrase: "Voicetypr", spoken_form: "voice typer", language: null, enabled: true },
      ]),
    );
    expect(invoke).toHaveBeenCalledWith("set_dictionary_suggestion_status", {
      id: "s1",
      status: "accepted",
    });
    expect(screen.queryByRole("region", { name: "Suggested words" })).not.toBeInTheDocument();
  });

  it("shows waiting suggestions in the Dictionary, where adding can be undone", async () => {
    const user = userEvent.setup();
    pending = [voicetypr];
    render(<DictionarySection />);
    const offered = within(await screen.findByRole("region", { name: "Suggested words" }));
    await user.click(offered.getByRole("button", { name: "Add Voicetypr to Dictionary" }));
    expect(await screen.findByRole("cell", { name: "Voicetypr" })).toBeInTheDocument();
    expect(screen.getByRole("cell", { name: "voice typer" })).toBeInTheDocument();
    expect(toast.success).toHaveBeenCalledWith(
      "Added “Voicetypr” to your Dictionary",
      expect.anything(),
    );

    undoLastToast();
    await waitFor(() => expect(useWritingSettings.getState().settings.custom_words).toEqual([]));
    expect(await screen.findByRole("region", { name: "Suggested words" })).toBeInTheDocument();
  });

  it("dismisses a suggestion without touching the Dictionary, and can bring it back", async () => {
    const user = userEvent.setup();
    pending = [voicetypr];
    render(<DictionarySection />);
    await user.click(await screen.findByRole("button", { name: "Dismiss Voicetypr" }));
    await waitFor(() =>
      expect(screen.queryByRole("region", { name: "Suggested words" })).not.toBeInTheDocument(),
    );
    expect(invoke).toHaveBeenCalledWith("set_dictionary_suggestion_status", {
      id: "s1",
      status: "dismissed",
    });
    expect(useWritingSettings.getState().settings.custom_words).toEqual([]);

    undoLastToast();
    expect(await screen.findByRole("region", { name: "Suggested words" })).toBeInTheDocument();
  });
it("keeps a capitals-only fix as a sounds-like form so every engine spells it", async () => {
    const user = userEvent.setup();
    pending = [{ id: "s2", heard: "github", word: "GitHub", status: "pending" }];
    render(<DictionarySection />);
    await user.click(await screen.findByRole("button", { name: "Add GitHub to Dictionary" }));
    await waitFor(() =>
      expect(useWritingSettings.getState().settings.custom_words).toEqual([
        { phrase: "GitHub", spoken_form: "github", language: null, enabled: true },
      ]),
    );
  });

  it("leaves the suggestion waiting when the Dictionary can't be saved", async () => {
    const user = userEvent.setup();
    pending = [voicetypr];
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_writing_settings") return defaultWritingSettings;
      if (command === "list_dictionary_suggestions") return pending;
      if (command === "update_writing_settings") throw new Error("disk full");
      return null;
    });
    render(<DictionarySection />);
    await user.click(await screen.findByRole("button", { name: "Add Voicetypr to Dictionary" }));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(useWritingSettings.getState().settings.custom_words).toEqual([]);
    expect(invoke).not.toHaveBeenCalledWith("set_dictionary_suggestion_status", expect.anything());
    expect(screen.getByRole("region", { name: "Suggested words" })).toBeInTheDocument();
  });

  it("undo removes the learned word even after the row was edited", async () => {
    const user = userEvent.setup();
    pending = [voicetypr];
    render(<DictionarySection />);
    await user.click(await screen.findByRole("button", { name: "Add Voicetypr to Dictionary" }));
    await screen.findByRole("cell", { name: "Voicetypr" });
    await user.click(screen.getByRole("button", { name: "Edit row 1" }));
    await user.type(screen.getByLabelText("Language"), "en");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(useWritingSettings.getState().settings.custom_words[0]?.language).toBe("en");
    undoLastToast();
    await waitFor(() => expect(useWritingSettings.getState().settings.custom_words).toEqual([]));
  });
});
