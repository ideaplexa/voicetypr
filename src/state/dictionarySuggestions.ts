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
  undo: (current: WritingSettings) => Partial<WritingSettings>;
};

// A corrected word becomes a Dictionary word with what was heard as its
// "sounds like" form. If the word already has a different one, the fix is
// kept as an exact correction instead.
function learn(settings: WritingSettings, { heard, word }: DictionarySuggestion): Change {
  const caseOnly = heard.toLowerCase() === word.toLowerCase();
  const existing = settings.custom_words.find((entry) => entry.phrase === word && !entry.language);
  if (!existing) {
    const added: CustomWord = {
      phrase: word,
      spoken_form: caseOnly ? null : heard,
      language: null,
      enabled: true,
    };
    return {
      apply: { custom_words: [...settings.custom_words, added] },
      undo: (current) => ({
        custom_words: current.custom_words.filter((entry) => entry !== added),
      }),
    };
  }
  if (caseOnly) return { apply: {}, undo: () => ({}) };
  if (!existing.spoken_form) {
    const updated: CustomWord = { ...existing, spoken_form: heard };
    return {
      apply: {
        custom_words: settings.custom_words.map((entry) => (entry === existing ? updated : entry)),
      },
      undo: (current) => ({
        custom_words: current.custom_words.map((entry) => (entry === updated ? existing : entry)),
      }),
    };
  }
  const rule: TextReplacementRule = { from: heard, to: word, language: null, enabled: true };
  return {
    apply: { replacements: [...settings.replacements, rule] },
    undo: (current) => ({ replacements: current.replacements.filter((entry) => entry !== rule) }),
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
      try {
        await setStatus(suggestion, "accepted");
      } catch (error) {
        toast.error(getErrorMessage(error, "Couldn't add the word"));
        return;
      }
      const change = learn(useWritingSettings.getState().settings, suggestion);
      writing.update(change.apply);
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
