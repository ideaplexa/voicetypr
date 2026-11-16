import { Sparkles } from "lucide-react";
import { Button } from "@/components/settings/SettingsButton";
import { useDictionarySuggestions } from "@/state/dictionarySuggestions";

/** Words the user corrected in History, offered for the Dictionary. `only` narrows the list to the given suggestion ids. */
export function DictionarySuggestions({ only }: { only?: string[] }) {
  const suggestions = useDictionarySuggestions((state) => state.suggestions);
  const accept = useDictionarySuggestions((state) => state.accept);
  const dismiss = useDictionarySuggestions((state) => state.dismiss);
  const shown = only ? suggestions.filter((entry) => only.includes(entry.id)) : suggestions;
  if (shown.length === 0) return null;
  return (
    <section
      aria-label="Suggested words"
      className="rounded-[14px] border border-border bg-card px-4 py-3"
    >
      <h2 className="inline-flex items-center gap-[6px] text-[10.5px] font-semibold uppercase tracking-[0.5px] text-muted-foreground">
        <Sparkles className="size-3" aria-hidden="true" />
        From your corrections
      </h2>
      <ul className="mt-1 divide-y divide-border">
        {shown.map((suggestion) => (
          <li key={suggestion.id} className="flex flex-wrap items-center gap-x-3 gap-y-1.5 py-2">
            <p className="min-w-0 flex-1 text-[13px] text-foreground">
              <span className="font-medium">{suggestion.word}</span>{" "}
              <span className="text-xs text-muted-foreground">
                {suggestion.heard.toLowerCase() === suggestion.word.toLowerCase()
                  ? `instead of “${suggestion.heard}”`
                  : `when Voicetypr hears “${suggestion.heard}”`}
              </span>
            </p>
            <div className="flex gap-1.5">
              <Button
                size="sm"
                aria-label={`Add ${suggestion.word} to Dictionary`}
                onClick={() => void accept(suggestion)}
              >
                Add to Dictionary
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Dismiss ${suggestion.word}`}
                onClick={() => void dismiss(suggestion)}
              >
                Dismiss
              </Button>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
