import { EnhancementSettings } from "@/components/EnhancementSettings";
import { ProviderSetupCard } from "@/components/polish/ProviderSetupCard";
import type { ProviderSetupCardProps } from "@/components/polish/ProviderSetupCard";
import type { EnhancementPreset } from "@/types/ai";
import type { WritingSettings } from "@/types/writing";

export function EnhancementSettingsPanel({
  preset,
  finalTextLanguage,
  writingSettings,
  aiFormattingEnabled,
  translationAvailable,
  writingSettingsDisabled,
  providerSetup,
  onPresetChange,
  onFinalTextLanguageChange,
  onWritingSettingsChange,
}: {
  preset: EnhancementPreset;
  finalTextLanguage: string;
  writingSettings: WritingSettings;
  aiFormattingEnabled: boolean;
  translationAvailable: boolean;
  writingSettingsDisabled: boolean;
  providerSetup: ProviderSetupCardProps;
  onPresetChange: (preset: EnhancementPreset) => void;
  onFinalTextLanguageChange: (value: string) => void;
  onWritingSettingsChange: (patch: Partial<WritingSettings>) => void;
}) {
  return (
    <EnhancementSettings
      preset={preset}
      finalTextLanguage={finalTextLanguage}
      writingSettings={writingSettings}
      aiFormattingEnabled={aiFormattingEnabled}
      translationAvailable={translationAvailable}
      providerContent={<ProviderSetupCard {...providerSetup} />}
      onPresetChange={onPresetChange}
      onFinalTextLanguageChange={onFinalTextLanguageChange}
      onWritingSettingsChange={onWritingSettingsChange}
      writingSettingsDisabled={writingSettingsDisabled}
    />
  );
}
