import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { create } from "zustand";
import { useWritingSettings } from "@/state/writingSettings";
import type { CustomWord, TextReplacementRule, WritingSettings } from "@/types/writing";
import { getErrorMessage } from "@/utils/error";

/** A word the user corrected in History, waiting to be added to the Dictionary. Local only. */
export interface DictionarySuggestion {
  id: string;
  heard: string;
  word: string;
  status: "pending" | "accepted" | "dismissed";
}

type Change = {
  apply: Partial<WritingSettings>;
  /** Whether the learned spelling is in the Dictionary, found by its fields so later edits to the row still count. */
  present: (current: WritingSettings) => boolean;
  /** Removes only the learned spelling and keeps any other change made since. */
  undo: (current: WritingSettings) => Partial<WritingSettings>;
};

const learnedWord = (entry: CustomWord, { heard, word }: DictionarySuggestion) =>
  entry.phrase === word && entry.spoken_form?.toLowerCase() === heard.toLowerCase();

// A corrected word becomes a Dictionary word with what was heard as its
// "sounds like" form, so the spelling is fixed after every engine. If the word
// already has a different sounds-like form, the fix is kept as a correction.
function learn(settings: WritingSettings, suggestion: DictionarySuggestion): Change {
  const { heard, word } = suggestion;
  const existing = settings.custom_words.find((entry) => entry.phrase === word && !entry.language);
  if (!existing || !existing.spoken_form) {
    const added: CustomWord = { phrase: word, spoken_form: heard, language: null, enabled: true };
    return {
      apply: {
        custom_words: existing
          ? settings.custom_words.map((entry) =>
              entry === existing ? { ...existing, spoken_form: heard } : entry,
            )
          : [...settings.custom_words, added],
      },
      present: (current) => current.custom_words.some((entry) => learnedWord(entry, suggestion)),
      undo: (current) => ({
        custom_words: existing
          ? current.custom_words.map((entry) =>
              learnedWord(entry, suggestion) ? { ...entry, spoken_form: null } : entry,
            )
          : current.custom_words.filter((entry) => !learnedWord(entry, suggestion)),
      }),
    };
  }
  const learnedRule = (rule: TextReplacementRule) =>
    rule.to === word && rule.from.toLowerCase() === heard.toLowerCase();
  return {
    apply: {
      replacements: [...settings.replacements, { from: heard, to: word, language: null, enabled: true }],
    },
    present: (current) => current.replacements.some(learnedRule),
    undo: (current) => ({ replacements: current.replacements.filter((rule) => !learnedRule(rule)) }),
  };
}

type DictionarySuggestionsState = {
  suggestions: DictionarySuggestion[];
  load: () => Promise<void>;
  offer: (offered: DictionarySuggestion[]) => void;
  accept: (suggestion: DictionarySuggestion) => Promise<void>;
  dismiss: (suggestion: DictionarySuggestion) => Promise<void>;
};

export const useDictionarySuggestions = create<DictionarySuggestionsState>((set, get) => {
  const setStatus = async (
    suggestion: DictionarySuggestion,
    status: DictionarySuggestion["status"],
  ) => {
    await invoke("set_dictionary_suggestion_status", { id: suggestion.id, status });
    const others = get().suggestions.filter((entry) => entry.id !== suggestion.id);
    set({
      suggestions: status === "pending" ? [...others, { ...suggestion, status }] : others,
    });
  };
  const restore = (suggestion: DictionarySuggestion) =>
    setStatus(suggestion, "pending").catch((error) =>
      toast.error(getErrorMessage(error, "Couldn't restore the suggestion")),
    );

  return {
    suggestions: [],
    load: async () => {
      try {
        set({
          suggestions: (await invoke<DictionarySuggestion[]>("list_dictionary_suggestions")) ?? [],
        });
      } catch {
        // Suggestions are optional: the Dictionary works without them.
      }
    },
    offer: (offered) => {
      const known = new Set(get().suggestions.map((entry) => entry.id));
      set({
        suggestions: [...get().suggestions, ...offered.filter((entry) => !known.has(entry.id))],
      });
    },
    accept: async (suggestion) => {
      const writing = useWritingSettings.getState();
      if (!(await writing.load())) {
        toast.error("Couldn't open your Dictionary");
        return;
      }
      const change = learn(useWritingSettings.getState().settings, suggestion);
      writing.update(change.apply);
      await writing.flush();
      if (!change.present(useWritingSettings.getState().settings)) return;
      try {
        await setStatus(suggestion, "accepted");
      } catch (error) {
        writing.update(change.undo(useWritingSettings.getState().settings));
        toast.error(getErrorMessage(error, "Couldn't add the word"));
        return;
      }
      toast.success(`Added “${suggestion.word}” to your Dictionary`, {
        action: {
          label: "Undo",
          onClick: () => {
            writing.update(change.undo(useWritingSettings.getState().settings));
            void restore(suggestion);
          },
        },
      });
    },
    dismiss: async (suggestion) => {
      try {
        await setStatus(suggestion, "dismissed");
      } catch (error) {
        toast.error(getErrorMessage(error, "Couldn't dismiss the suggestion"));
        return;
      }
      toast.success("Suggestion dismissed", {
        action: { label: "Undo", onClick: () => void restore(suggestion) },
      });
    },
  };
});
