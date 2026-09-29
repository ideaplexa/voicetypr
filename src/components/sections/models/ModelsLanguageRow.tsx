import { LanguageSelection } from "@/components/LanguageSelection";
import type { SpeechModelEngine } from "@/types";

interface ModelsLanguageRowProps {
  languageValue: string;
  currentEngine: SpeechModelEngine;
  isEnglishOnlyModel: boolean;
  supportedLanguages?: readonly string[];
  onLanguageChange: (value: string) => void;
}

export function ModelsLanguageRow({
  languageValue,
  currentEngine,
  isEnglishOnlyModel,
  supportedLanguages,
  onLanguageChange,
}: ModelsLanguageRowProps) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-4">
      <div className="min-w-0">
        <h2 className="text-[13.5px] font-medium leading-[normal]">Spoken language</h2>
        <p className="mt-0.5 text-xs leading-[normal] text-muted-foreground">
          {isEnglishOnlyModel ? "This model uses English only." : "What you'll speak."}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <LanguageSelection
          className="h-auto w-auto min-w-[100px] rounded-[10px] px-2.5 py-2 text-[13px] leading-[normal]"
          value={languageValue}
          engine={currentEngine}
          englishOnly={isEnglishOnlyModel}
          supportedLanguages={supportedLanguages}
          onValueChange={(value) => void onLanguageChange(value)}
        />
      </div>
    </div>
  );
}
