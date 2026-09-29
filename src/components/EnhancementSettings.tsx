import { LanguageSelection } from "@/components/LanguageSelection";
import { Button } from "@/components/settings/SettingsButton";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/settings/SettingsSwitch";
import { Segmented, SettingRow, SettingsCard } from "@/components/settings/settings-ui";
import { presetRequiresAiFormatting, type EnhancementPreset } from "@/types/ai";
import type { AppFormattingRule, WritingSettings } from "@/types/writing";
import { ArrowRight, Plus, Trash2 } from "lucide-react";
import type { ReactNode } from "react";

interface EnhancementSettingsProps {
  preset: EnhancementPreset;
  finalTextLanguage: string;
  writingSettings: WritingSettings;
  aiFormattingEnabled: boolean;
  providerContent: ReactNode;
  onPresetChange: (value: EnhancementPreset) => void;
  onFinalTextLanguageChange: (value: string) => void;
  onWritingSettingsChange: (patch: Partial<WritingSettings>) => void;
  disabled?: boolean;
  writingSettingsDisabled?: boolean;
}

const modes: { value: EnhancementPreset; label: string; example: string }[] = [
  { value: "PersonalDictation", label: "Off", example: "so um I think we should uh meet on Tuesday no wait Wednesday and bring the draft" },
  { value: "CleanDictation", label: "Clean", example: "I think we should meet on Wednesday and bring the draft." },
  { value: "Writing", label: "Writing", example: "Let's meet on Wednesday to review the draft." },
  { value: "Notes", label: "Notes", example: "Meeting: Wednesday. Bring the draft." },
  { value: "Message", label: "Message", example: "Let's meet Wednesday. Please bring the draft!" },
  { value: "Code", label: "Code", example: "// Meet on Wednesday and bring the draft." },
];

export function EnhancementSettings({
  preset, finalTextLanguage, writingSettings, aiFormattingEnabled, providerContent,
  onPresetChange, onFinalTextLanguageChange, onWritingSettingsChange,
  disabled = false, writingSettingsDisabled = disabled,
}: EnhancementSettingsProps) {
  const rules = writingSettings.app_formatting_rules;
  const selectedExample = modes.find((mode) => mode.value === preset)?.example ?? modes[0].example;
  const updateRule = (index: number, patch: Partial<AppFormattingRule>) => {
    onWritingSettingsChange({
      app_formatting_rules: rules.map((rule, current) => current === index ? { ...rule, ...patch } : rule),
    });
  };
  const languageEnabled = preset !== "PersonalDictation";
  const usingSpecificLanguage = languageEnabled && finalTextLanguage !== "same_as_transcript";

  return <div className="flex flex-col gap-[18px]">
    <section aria-label="Modes">
      <SettingsCard title="Default style" action={
        <div className="max-w-full overflow-x-auto">
          <Segmented label="Default style" value={preset} onValueChange={(value) => onPresetChange(value as EnhancementPreset)}
            options={modes.map((mode) => ({ value: mode.value, label: mode.label,
              disabled: disabled || (!aiFormattingEnabled && presetRequiresAiFormatting(mode.value) && preset !== mode.value) }))} />
        </div>
      }>
        <div className="mt-[14px] grid items-stretch gap-3 sm:grid-cols-[1fr_auto_1fr]">
          <div className="rounded-[10px] bg-muted p-3.5">
            <p className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">You said</p>
            <p className="mt-1.5 text-[13px] leading-[19px] text-muted-foreground">so um I think we should uh meet on Tuesday no wait Wednesday and bring the draft</p>
          </div>
          <ArrowRight className="hidden size-4 self-center text-muted-foreground sm:block" aria-hidden="true" />
          <div className="rounded-[10px] bg-sage-bg p-3.5">
            <p className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">You get</p>
            <p className="mt-1.5 text-[13px] leading-[19px] text-muted-foreground">{selectedExample}</p>
          </div>
        </div>
        <p className="mt-2 text-xs text-muted-foreground">Illustrative example; your results depend on the selected provider.</p>
      </SettingsCard>
    </section>

    <div className="grid gap-[14px] md:grid-cols-[minmax(0,1fr)_300px]">
      <section aria-label="Provider">
        <SettingsCard title="AI provider" className="h-full">
          <div className="mt-3">{providerContent}</div>
        </SettingsCard>
      </section>
      <SettingsCard title="Per-app styles" className="min-w-0" action={
        <Button size="sm" variant="ghost" aria-label="Add override" disabled={writingSettingsDisabled}
          onClick={() => onWritingSettingsChange({ app_formatting_rules: [...rules, { app_name: "", preset, enabled: true }] })}>
          <Plus className="size-4" /> Add
        </Button>
      }>
        {rules.length === 0 ? <p className="mt-3 text-xs text-muted-foreground">No app overrides yet. Add an app to use a different style.</p> : null}
        <div className="mt-3 space-y-2">
          {rules.map((rule, index) => <div key={index} className="flex items-center gap-2 rounded-[8px] bg-muted px-2.5 py-2">
              <input aria-label={`App name ${index + 1}`} placeholder="App name, e.g. Slack" value={rule.app_name}
                disabled={writingSettingsDisabled} onChange={(event) => updateRule(index, { app_name: event.target.value })}
                className="w-0 min-w-0 flex-1 bg-transparent text-[13px] font-medium text-foreground outline-none placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring" />
              <Select value={rule.preset} disabled={writingSettingsDisabled} onValueChange={(value) => updateRule(index, { preset: value as EnhancementPreset })}>
                <SelectTrigger size="sm" aria-label={`Style for ${rule.app_name || `app ${index + 1}`}`} className="h-7 w-auto shrink-0 rounded-md border-0 bg-transparent px-1 text-xs font-semibold text-sage shadow-none"><SelectValue>{modes.find((mode) => mode.value === rule.preset)?.label ?? rule.preset}</SelectValue></SelectTrigger>
                <SelectContent>{modes.map((mode) => <SelectItem key={mode.value} value={mode.value}
                  disabled={!aiFormattingEnabled && presetRequiresAiFormatting(mode.value) && rule.preset !== mode.value}>{mode.label}</SelectItem>)}</SelectContent>
              </Select>
              <Switch aria-label={`Enable ${rule.app_name || `app ${index + 1}`} style`} checked={rule.enabled}
                disabled={writingSettingsDisabled} onCheckedChange={(enabled) => updateRule(index, { enabled })} />
              <Button variant="ghost" size="icon-sm" aria-label={`Delete ${rule.app_name || `app ${index + 1}`} style`} disabled={writingSettingsDisabled}
                onClick={() => onWritingSettingsChange({ app_formatting_rules: rules.filter((_, current) => current !== index) })}><Trash2 className="size-3.5" /></Button>
          </div>)}
        </div>
        {!aiFormattingEnabled && rules.some((rule) => presetRequiresAiFormatting(rule.preset)) ?
          <p className="mt-3 text-xs text-muted-foreground">Turn on Polish to activate app styles that use AI.</p> : null}
      </SettingsCard>
    </div>

    <SettingsCard title="More">
      <SettingRow title={<span className="text-muted-foreground">Final text language</span>} description="Keep the transcript language, or choose a different written language. Translation requires Polish."
        control={<div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant={!usingSpecificLanguage ? "default" : "outline"} disabled={disabled}
            onClick={() => onFinalTextLanguageChange("same_as_transcript")}>Same as transcript</Button>
          <Button size="sm" variant={usingSpecificLanguage ? "default" : "outline"} disabled={disabled || !languageEnabled}
            onClick={() => onFinalTextLanguageChange(usingSpecificLanguage ? finalTextLanguage : "en")}>Specific language</Button>
        </div>} />
      {usingSpecificLanguage ? <div className="mt-3"><LanguageSelection value={finalTextLanguage} onValueChange={onFinalTextLanguageChange} className="w-full md:w-64" /></div> : null}
      {!aiFormattingEnabled && finalTextLanguage !== "same_as_transcript" ? <p className="mt-2 text-xs text-muted-foreground">Turn on Polish to use a different final text language.</p> : null}
    </SettingsCard>
  </div>;
}
