import { LanguageSelection } from "@/components/LanguageSelection";
import { Switch } from "@/components/settings/SettingsSwitch";
import { Spinner } from "@/components/ui/spinner";
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import type { SpeechModelEngine } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useId, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { supportsFillerRemoval, supportsParagraphs } from "./modelOptionRules";
import { useRealtimeWords } from "./useRealtimeWords";

const log = createLogger("models");

function OptionRow({
  title,
  description,
  control,
}: {
  title: string;
  description: string;
  control: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-4 py-2.5">
      <div className="min-w-0">
        <p className="text-[13px] font-medium leading-[normal] text-foreground">{title}</p>
        <p className="mt-0.5 text-xs leading-[normal] text-muted-foreground">{description}</p>
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  );
}

function usePolishOn(): boolean {
  const [on, setOn] = useState(false);
  useEffect(() => {
    let active = true;
    void invoke<{ enabled?: boolean } | null>("get_ai_settings")
      .then((value) => {
        if (active) setOn(value?.enabled === true);
      })
      .catch(() => {
        if (active) setOn(false);
      });
    return () => {
      active = false;
    };
  }, []);
  return on;
}

interface ModelOptionsProps {
  engine: SpeechModelEngine;
  modelName: string;
  languageValue: string;
  isEnglishOnlyModel: boolean;
  supportedLanguages?: readonly string[];
  onLanguageChange: (value: string) => void;
}

/** Options for the selected model, shown only where the model supports them. */
export function ModelOptions({
  engine,
  modelName,
  languageValue,
  isEnglishOnlyModel,
  supportedLanguages,
  onLanguageChange,
}: ModelOptionsProps) {
  const { settings, updateSettings } = useSettings();
  const realtime = useRealtimeWords(engine, modelName);
  const polishOn = usePolishOn();
  const baseId = useId();
  const showParagraphs = supportsParagraphs(engine, modelName);
  // Deepgram's WebSocket API has no paragraphs parameter/paragraph transcript.
  // Keep the saved batch preference, but don't offer an ineffective stream switch.
  const paragraphsUnavailable = engine === "deepgram" && settings?.transcription_mode === "live_preview";
  const showFillers = supportsFillerRemoval(languageValue, isEnglishOnlyModel);

  const save = (patch: { transcript_paragraphs?: boolean; transcript_remove_fillers?: boolean }) => {
    void updateSettings(patch).catch((error) => {
      log.error("Failed to update transcript option:", error);
      toast.error("Failed to update setting");
    });
  };

  return (
    <div
      data-testid="model-options"
      className="col-span-full divide-y divide-sage/15 border-t border-sage/15 pl-7"
    >
      <OptionRow
        title="Language"
        description={
          isEnglishOnlyModel ? "This model speaks English only." : "What you'll speak with this model"
        }
        control={
          isEnglishOnlyModel ? (
            <span className="rounded-md bg-muted px-2 py-1 text-xs text-muted-foreground">
              English only
            </span>
          ) : (
            <LanguageSelection
              className="h-auto w-auto min-w-[100px] rounded-[10px] px-2.5 py-2 text-[13px] leading-[normal]"
              value={languageValue}
              engine={engine}
              englishOnly={false}
              supportedLanguages={supportedLanguages}
              onValueChange={(value) => void onLanguageChange(value)}
            />
          )
        }
      />
      {realtime.streams ? (
        <OptionRow
          title="Real-time words"
          description="Transcribe while you talk — final text arrives faster"
          control={
            <span className="flex items-center gap-2">
              {realtime.isActivating ? <Spinner className="size-3" /> : null}
              <Switch
                id={`${baseId}-realtime`}
                aria-label="Real-time words"
                checked={realtime.realtimeOn}
                disabled={realtime.isActivating}
                onCheckedChange={(checked) => void realtime.setRealtime(checked)}
              />
            </span>
          }
        />
      ) : null}
      {showParagraphs ? (
        <OptionRow
          title="Paragraphs at long pauses"
          description={
            paragraphsUnavailable
              ? "Deepgram paragraphs need Real-time words off."
              : "Starts a new paragraph when you pause on long dictations"
          }
          control={
            <Switch
              id={`${baseId}-paragraphs`}
              aria-label="Paragraphs at long pauses"
              checked={settings?.transcript_paragraphs ?? true}
              disabled={paragraphsUnavailable}
              onCheckedChange={(checked) => save({ transcript_paragraphs: checked })}
            />
          }
        />
      ) : null}
      {showFillers ? (
        <OptionRow
          title="Remove filler words"
          description="um, uh, and repeated words"
          control={
            <Switch
              id={`${baseId}-fillers`}
              aria-label="Remove filler words"
              checked={settings?.transcript_remove_fillers ?? false}
              onCheckedChange={(checked) => save({ transcript_remove_fillers: checked })}
            />
          }
        />
      ) : null}
      {polishOn && (showParagraphs || showFillers) ? (
        <p className="py-2 text-xs text-muted-foreground">Polish already handles this.</p>
      ) : null}
    </div>
  );
}
