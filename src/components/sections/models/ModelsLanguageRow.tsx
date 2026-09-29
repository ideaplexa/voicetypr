import { LanguageSelection } from "@/components/LanguageSelection";
import { Badge } from "@/components/ui/badge";
import { Spinner } from "@/components/ui/spinner";
import type { SpeechModelEngine } from "@/types";
import { Download } from "lucide-react";

interface ModelsLanguageRowProps {
  languageValue: string;
  currentEngine: SpeechModelEngine;
  isEnglishOnlyModel: boolean;
  supportedLanguages?: readonly string[];
  hasDownloading: boolean;
  hasVerifying: boolean;
  onLanguageChange: (value: string) => void;
}

export function ModelsLanguageRow({
  languageValue,
  currentEngine,
  isEnglishOnlyModel,
  supportedLanguages,
  hasDownloading,
  hasVerifying,
  onLanguageChange,
}: ModelsLanguageRowProps) {
  return (
    <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
      <p className="text-xs text-muted-foreground">
        {isEnglishOnlyModel ? "This model uses English only." : "What you'll speak."}
      </p>
      <div className="flex items-center gap-2">
        {hasDownloading || hasVerifying ? (
          <Badge variant="outline" className="gap-1.5 bg-sage-bg text-sage">
            {hasDownloading ? <Download className="size-3.5" /> : <Spinner className="size-3.5" />}
            {hasDownloading ? "Downloading…" : "Verifying…"}
          </Badge>
        ) : null}
        <LanguageSelection
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
