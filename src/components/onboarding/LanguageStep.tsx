import { languages } from "@/components/languages";
import { FrameHeading, PrimaryButton } from "@/components/onboarding/Frame";
import { cn } from "@/lib/utils";
import { Check, Search } from "lucide-react";
import { useState } from "react";

const FIRST = ["en", "es", "fr", "de", "hi", "ja"];
const ORDERED = [
  ...FIRST.flatMap((code) => languages.filter((language) => language.value === code)),
  ...languages.filter((language) => !FIRST.includes(language.value)),
];

/** 03 Which language do you speak: it decides which models fit. */
export function LanguageStep({
  language,
  busy,
  onLanguageChange,
  onNext,
}: {
  language: string;
  busy: boolean;
  onLanguageChange: (value: string) => void;
  onNext: () => void;
}) {
  const [query, setQuery] = useState("");
  const shown = ORDERED.filter((item) => item.label.toLowerCase().includes(query.trim().toLowerCase()));
  return (
    <section className="flex w-[480px] flex-col items-center gap-7">
      <FrameHeading title="Which language do you speak?" line="Voicetypr suggests the models that understand it best." />
      <div className="flex flex-col self-stretch overflow-hidden rounded-[14px] border border-border bg-card">
        <label className="flex h-[50px] shrink-0 items-center gap-3.5 border-b border-border px-[18px]">
          <Search aria-hidden="true" className="size-4 text-text-3" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search language..."
            aria-label="Search language"
            className="min-w-0 flex-1 bg-transparent text-[15px] outline-none"
          />
        </label>
        <div role="listbox" aria-label="Language" className="max-h-[288px] overflow-y-auto">
          {shown.map((item) => {
            const on = item.value === language;
            return (
              <button
                key={item.value}
                type="button"
                role="option"
                aria-selected={on}
                onClick={() => onLanguageChange(item.value)}
                className={cn(
                  "flex h-12 w-full items-center gap-3.5 border-b border-border px-[18px] text-left text-[15.5px] text-foreground last:border-b-0",
                  on ? "bg-sage-bg font-semibold" : "hover:bg-accent",
                )}
              >
                <span className="min-w-0 flex-1">{item.label}</span>
                {on ? <Check aria-hidden="true" className="size-[17px] text-sage" /> : null}
              </button>
            );
          })}
        </div>
      </div>
      <PrimaryButton disabled={busy} onClick={onNext}>Continue</PrimaryButton>
    </section>
  );
}
