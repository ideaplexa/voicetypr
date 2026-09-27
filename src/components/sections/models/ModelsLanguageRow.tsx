import { LanguageSelection } from "@/components/LanguageSelection";
import { Badge } from "@/components/ui/badge";
import { SettingRow } from "@/components/settings/settings-ui";
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
    <SettingRow
      title="Spoken language"
      description="The language you speak. English-only models lock this to English."
      control={
        <div className="flex items-center gap-2">
          {(hasDownloading || hasVerifying) && (
            <Badge variant="outline" className="gap-1.5 bg-primary/10 text-primary">
              {hasDownloading ? (
                <Download className="size-3.5" />
              ) : (
                <Spinner className="size-3.5" />
              )}
              {hasDownloading ? "Downloading…" : "Verifying…"}
            </Badge>
          )}
          <LanguageSelection
            value={languageValue}
            engine={currentEngine}
            englishOnly={isEnglishOnlyModel}
            supportedLanguages={supportedLanguages}
            onValueChange={(value) => void onLanguageChange(value)}
          />
        </div>
      }
    />
  );
}
