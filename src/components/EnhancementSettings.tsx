import { LanguageSelection } from "@/components/LanguageSelection";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
  InputGroupText,
  InputGroupTextarea,
} from "@/components/ui/input-group";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { presetDisplayLabel, presetRequiresAiFormatting, type EnhancementPreset } from "@/types/ai";
import type {
  AppFormattingRule,
  CustomWord,
  Snippet,
  TextReplacementRule,
  WritingSettings,
} from "@/types/writing";
import { Globe, Plus, Trash2 } from "lucide-react";
import { useState, type ReactNode } from "react";

interface EnhancementSettingsProps {
  preset: EnhancementPreset;
  finalTextLanguage: string;
  writingSettings: WritingSettings;
  aiFormattingEnabled: boolean;
  providerContent: ReactNode;
  onPresetChange: (value: EnhancementPreset) => void;
  onFinalTextLanguageChange: (value: string) => void;
  onWritingSettingsChange: (settings: WritingSettings) => void;
  disabled?: boolean;
  writingSettingsDisabled?: boolean;
}

function updateItem<T>(items: T[], index: number, next: T): T[] {
  return items.map((item, itemIndex) => (itemIndex === index ? next : item));
}

function removeItem<T>(items: T[], index: number): T[] {
  return items.filter((_, itemIndex) => itemIndex !== index);
}

const EMPTY_OBJECT_LIST: readonly object[] = [];
const EMPTY_KEY_LIST: string[] = [];

function useStableRowKeys(items: readonly object[]): string[] {
  // Stable React keys for editable rows whose item references change on every
  // keystroke. Keys are derived from the previous list (slot-aligned reuse),
  // adjusted during render via state — no refs, safe under concurrent React.
  const [state, setState] = useState<{
    prev: readonly object[];
    keys: string[];
    nextId: number;
  }>({ prev: EMPTY_OBJECT_LIST, keys: EMPTY_KEY_LIST, nextId: 0 });

  let keys = state.keys;
  if (state.prev !== items) {
    const used = new Set<object>(items);
    const taken = new Set<string>();
    const nextKeys: string[] = [];
    let nextId = state.nextId;
    for (let index = 0; index < items.length; index++) {
      const displaced = state.prev[index];
      const slotReusable =
        displaced !== undefined && displaced !== items[index] && !used.has(displaced);
      let key =
        state.prev[index] === items[index]
          ? state.keys[index]
          : slotReusable
            ? state.keys[index]
            : undefined;
      if (!key || taken.has(key)) {
        key = `row-${nextId++}`;
      }
      taken.add(key);
      nextKeys.push(key);
    }
    keys = nextKeys;
    setState({ prev: items, keys: nextKeys, nextId });
  }
  return keys;
}
const FORMATTING_MODES = [
  { id: "PersonalDictation" },
  { id: "CleanDictation" },
  { id: "Writing" },
  { id: "Notes" },
  { id: "Message" },
  { id: "Code" },
] as const satisfies ReadonlyArray<{
  id: EnhancementPreset;
}>;

const formattingModeLabel = (preset: EnhancementPreset) => presetDisplayLabel(preset);

function AppFormattingRulesEditor({
  preset,
  rules,
  onChange,
  disabled,
  aiFormattingEnabled,
}: {
  preset: EnhancementPreset;
  rules: AppFormattingRule[];
  onChange: (rules: AppFormattingRule[]) => void;
  disabled: boolean;
  aiFormattingEnabled: boolean;
}) {
  const rowKeys = useStableRowKeys(rules);
  const hasAiRequiredSelection =
    !aiFormattingEnabled && rules.some((rule) => presetRequiresAiFormatting(rule.preset));

  return (
    <FieldSet className="mt-4 rounded-lg border border-border/60 bg-background/60 p-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1 text-sm">Per-app modes</FieldLegend>
          <FieldDescription>
            Override the default mode when dictation starts in a matched app. App identity is
            captured locally for every desktop transcription.
          </FieldDescription>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() => onChange([...rules, { app_name: "", preset, enabled: true }])}
        >
          <Plus className="mr-2 h-4 w-4" />
          Add override
        </Button>
      </div>

      {!aiFormattingEnabled && hasAiRequiredSelection && (
        <div className="mt-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
          One or more overrides need Polish. Turn on Polish to activate them.
        </div>
      )}

      {rules.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-4">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No app overrides yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example: use Message mode whenever Slack is active.
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-2">
          {rules.map((rule, index) => {
            const selectedMode = FORMATTING_MODES.find((mode) => mode.id === rule.preset);

            return (
              <div key={rowKeys[index]} className="rounded-lg border border-border bg-card p-3">
                <div className="flex flex-wrap items-center gap-2">
                  <InputGroup className="min-w-[10rem] flex-1">
                    <InputGroupAddon>
                      <InputGroupText>App</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="App name, e.g. Slack"
                      value={rule.app_name}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(rules, index, {
                            ...rule,
                            app_name: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>

                  <Select
                    value={rule.preset}
                    disabled={disabled}
                    onValueChange={(value) =>
                      onChange(
                        updateItem(rules, index, {
                          ...rule,
                          preset: value as EnhancementPreset,
                        }),
                      )
                    }
                  >
                    <SelectTrigger size="sm" className="w-[11rem]" aria-label="App mode">
                      <SelectValue placeholder="Preset">
                        {selectedMode ? formattingModeLabel(selectedMode.id) : rule.preset}
                      </SelectValue>
                    </SelectTrigger>
                    <SelectContent>
                      {FORMATTING_MODES.map((modeOption) => {
                        const requiresAi = presetRequiresAiFormatting(modeOption.id);
                        const isSelected = rule.preset === modeOption.id;
                        const isOptionDisabled =
                          disabled || (requiresAi && !aiFormattingEnabled && !isSelected);

                        return (
                          <SelectItem
                            key={modeOption.id}
                            value={modeOption.id}
                            disabled={isOptionDisabled}
                          >
                            {formattingModeLabel(modeOption.id)}
                            {requiresAi && !aiFormattingEnabled && !isSelected
                              ? " (requires Polish)"
                              : ""}
                          </SelectItem>
                        );
                      })}
                    </SelectContent>
                  </Select>

                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground">Enabled</span>
                    <Switch
                      checked={rule.enabled}
                      disabled={disabled}
                      onCheckedChange={(checked) =>
                        onChange(updateItem(rules, index, { ...rule, enabled: checked }))
                      }
                    />
                    <Button
                      type="button"
                      size="icon"
                      variant="ghost"
                      disabled={disabled}
                      onClick={() => onChange(removeItem(rules, index))}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </div>
            );
          })}
        </FieldGroup>
      )}
    </FieldSet>
  );
}

function ReplacementEditor({
  replacements,
  onChange,
  disabled,
}: {
  replacements: TextReplacementRule[];
  onChange: (replacements: TextReplacementRule[]) => void;
  disabled: boolean;
}) {
  const rowKeys = useStableRowKeys(replacements);
  return (
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1">Corrections</FieldLegend>
          <FieldDescription>
            Always applied after transcription. Whisper and Soniox can also use them while
            recognizing speech.
          </FieldDescription>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() =>
            onChange([...replacements, { from: "", to: "", language: null, enabled: true }])
          }
        >
          <Plus className="mr-2 h-4 w-4" />
          Add rule
        </Button>
      </div>

      {replacements.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-6">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No replacement rules yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example: <span className="font-mono">voice typer → Voicetypr</span>
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-3">
          {replacements.map((rule, index) => (
            <FieldSet
              key={rowKeys[index]}
              className="rounded-lg border border-border/60 bg-background/60 p-3"
            >
              <div className="mb-3 flex items-center justify-between gap-3">
                <FieldTitle>Rule {index + 1}</FieldTitle>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">Enabled</span>
                  <Switch
                    checked={rule.enabled}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                      onChange(updateItem(replacements, index, { ...rule, enabled: checked }))
                    }
                  />
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    disabled={disabled}
                    onClick={() => onChange(removeItem(replacements, index))}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>

              <FieldGroup className="gap-3">
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Match</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Text to match"
                      value={rule.from}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(replacements, index, {
                            ...rule,
                            from: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Replace</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Replacement text"
                      value={rule.to}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(replacements, index, {
                            ...rule,
                            to: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <Globe className="h-4 w-4" />
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Language code (optional, e.g. en)"
                      value={rule.language ?? ""}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(replacements, index, {
                            ...rule,
                            language: event.target.value || null,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
              </FieldGroup>
            </FieldSet>
          ))}
        </FieldGroup>
      )}
    </FieldSet>
  );
}

function CustomWordEditor({
  customWords,
  onChange,
  disabled,
}: {
  customWords: CustomWord[];
  onChange: (customWords: CustomWord[]) => void;
  disabled: boolean;
}) {
  const rowKeys = useStableRowKeys(customWords);
  return (
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1">Words &amp; names</FieldLegend>
          <FieldDescription>
            Always corrects spelling. Whisper, Parakeet, Deepgram, and Soniox can also use these
            terms while recognizing speech.
          </FieldDescription>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() =>
            onChange([
              ...customWords,
              { phrase: "", spoken_form: null, language: null, enabled: true },
            ])
          }
        >
          <Plus className="mr-2 h-4 w-4" />
          Add word
        </Button>
      </div>

      {customWords.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-6">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No words or names yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example: canonical <span className="font-mono">Voicetypr</span>, spoken form{" "}
              <span className="font-mono">voice typer</span>
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-3">
          {customWords.map((word, index) => (
            <FieldSet
              key={rowKeys[index]}
              className="rounded-lg border border-border/60 bg-background/60 p-3"
            >
              <div className="mb-3 flex items-center justify-between gap-3">
                <FieldTitle>Word {index + 1}</FieldTitle>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">Enabled</span>
                  <Switch
                    checked={word.enabled}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                      onChange(updateItem(customWords, index, { ...word, enabled: checked }))
                    }
                  />
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    disabled={disabled}
                    onClick={() => onChange(removeItem(customWords, index))}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>

              <FieldGroup className="gap-3">
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Canonical</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Canonical phrase"
                      value={word.phrase}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(customWords, index, {
                            ...word,
                            phrase: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Spoken</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Spoken form (optional)"
                      value={word.spoken_form ?? ""}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(customWords, index, {
                            ...word,
                            spoken_form: event.target.value || null,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <Globe className="h-4 w-4" />
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="Language code (optional, e.g. en)"
                      value={word.language ?? ""}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(customWords, index, {
                            ...word,
                            language: event.target.value || null,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>
              </FieldGroup>
            </FieldSet>
          ))}
        </FieldGroup>
      )}
    </FieldSet>
  );
}

function SnippetEditor({
  snippets,
  onChange,
  disabled,
}: {
  snippets: Snippet[];
  onChange: (snippets: Snippet[]) => void;
  disabled: boolean;
}) {
  const rowKeys = useStableRowKeys(snippets);
  return (
    <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <FieldLegend className="mb-1">Saved text</FieldLegend>
          <FieldDescription>
            Say “insert” followed by a trigger to add a saved signature, address, response, or
            template.
          </FieldDescription>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() =>
            onChange([
              ...snippets,
              {
                trigger: "",
                body: "",
                language: null,
                enabled: true,
                preserve_literal: true,
              },
            ])
          }
        >
          <Plus className="mr-2 h-4 w-4" />
          Add saved text
        </Button>
      </div>

      {snippets.length === 0 ? (
        <Empty className="mt-3 border-border/60 bg-muted/20 p-6">
          <EmptyHeader className="max-w-none gap-1">
            <EmptyTitle className="text-sm">No saved text yet</EmptyTitle>
            <EmptyDescription className="text-xs">
              Example trigger: <span className="font-mono">insert my signature</span>
            </EmptyDescription>
          </EmptyHeader>
        </Empty>
      ) : (
        <FieldGroup className="mt-3 gap-3">
          {snippets.map((snippet, index) => (
            <FieldSet
              key={rowKeys[index]}
              className="rounded-lg border border-border/60 bg-background/60 p-3"
            >
              <div className="mb-3 flex items-center justify-between gap-3">
                <FieldTitle>Saved text {index + 1}</FieldTitle>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-muted-foreground">Enabled</span>
                  <Switch
                    checked={snippet.enabled}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                      onChange(updateItem(snippets, index, { ...snippet, enabled: checked }))
                    }
                  />
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
                    disabled={disabled}
                    onClick={() => onChange(removeItem(snippets, index))}
                  >
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </div>
              </div>

              <FieldGroup className="gap-3">
                <Field>
                  <InputGroup>
                    <InputGroupAddon>
                      <InputGroupText>Insert</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupInput
                      placeholder="my signature"
                      value={snippet.trigger.replace(/^insert\s+/i, "")}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(snippets, index, {
                            ...snippet,
                            trigger: event.target.value.trimStart()
                              ? `insert ${event.target.value.trimStart()}`
                              : "",
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>

                <Field>
                  <InputGroup>
                    <InputGroupAddon align="block-start">
                      <InputGroupText>Text to insert</InputGroupText>
                    </InputGroupAddon>
                    <InputGroupTextarea
                      placeholder="Saved text"
                      value={snippet.body}
                      disabled={disabled}
                      onChange={(event) =>
                        onChange(
                          updateItem(snippets, index, {
                            ...snippet,
                            body: event.target.value,
                          }),
                        )
                      }
                    />
                  </InputGroup>
                </Field>

                <Field orientation="responsive">
                  <FieldContent>
                    <InputGroup>
                      <InputGroupAddon>
                        <Globe className="h-4 w-4" />
                      </InputGroupAddon>
                      <InputGroupInput
                        placeholder="Language code (optional, e.g. en)"
                        value={snippet.language ?? ""}
                        disabled={disabled}
                        onChange={(event) =>
                          onChange(
                            updateItem(snippets, index, {
                              ...snippet,
                              language: event.target.value || null,
                            }),
                          )
                        }
                      />
                    </InputGroup>
                  </FieldContent>
                  <FieldLabel className="md:justify-end">
                    <Field orientation="horizontal">
                      <Switch
                        checked={snippet.preserve_literal}
                        disabled={disabled}
                        onCheckedChange={(checked) =>
                          onChange(
                            updateItem(snippets, index, {
                              ...snippet,
                              preserve_literal: checked,
                            }),
                          )
                        }
                      />
                      <FieldContent>
                        <FieldTitle className="text-xs">Keep text exact</FieldTitle>
                      </FieldContent>
                    </Field>
                  </FieldLabel>
                </Field>
              </FieldGroup>
            </FieldSet>
          ))}
        </FieldGroup>
      )}
    </FieldSet>
  );
}

export function EnhancementSettings({
  preset,
  finalTextLanguage,
  writingSettings,
  aiFormattingEnabled,
  providerContent,
  onPresetChange,
  onFinalTextLanguageChange,
  onWritingSettingsChange,
  disabled = false,
  writingSettingsDisabled = disabled,
}: EnhancementSettingsProps) {
  const allowsSpecificFinalLanguage = preset !== "PersonalDictation";
  const usingSpecificLanguage =
    allowsSpecificFinalLanguage && finalTextLanguage !== "same_as_transcript";

  return (
    <div className={disabled ? "flex flex-col gap-5 opacity-60" : "flex flex-col gap-5"}>
      <div className="flex min-w-0 max-w-4xl flex-col gap-5">
        <section aria-label="Provider">{providerContent}</section>

        <section aria-label="Modes" className="flex flex-col gap-5">
          <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
            <FieldLegend className="mb-1">Default mode</FieldLegend>
            <FieldDescription className="mb-3">
              Applied unless a per-app override matches.
            </FieldDescription>
            <Tabs
              value={preset}
              onValueChange={(value) => onPresetChange(value as EnhancementPreset)}
            >
              <TabsList
                aria-label="Default mode"
                className="grid h-auto w-full grid-cols-3 sm:grid-cols-6"
              >
                {FORMATTING_MODES.map((mode) => {
                  const requiresPolish =
                    !aiFormattingEnabled && presetRequiresAiFormatting(mode.id);
                  return (
                    <TabsTrigger
                      key={mode.id}
                      value={mode.id}
                      disabled={requiresPolish}
                      className="gap-1.5 px-2 py-2 text-xs"
                      title={requiresPolish ? "Requires Polish to be turned on" : undefined}
                    >
                      {formattingModeLabel(mode.id)}
                    </TabsTrigger>
                  );
                })}
              </TabsList>
            </Tabs>
          </FieldSet>

          <FieldSet className="rounded-xl border border-border/60 bg-card p-4">
            <FieldLegend className="mb-1">Final text language</FieldLegend>
            <FieldDescription className="mb-3">
              Keep the transcript language, or choose a different written language. Translation
              requires Polish.
            </FieldDescription>
            {!aiFormattingEnabled && finalTextLanguage !== "same_as_transcript" ? (
              <div className="mb-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
                Turn on Polish to use a different final text language.
              </div>
            ) : null}
            <ButtonGroup className="w-full flex-wrap md:w-fit">
              <Button
                type="button"
                variant={!usingSpecificLanguage ? "default" : "outline"}
                size="sm"
                disabled={disabled}
                onClick={() => onFinalTextLanguageChange("same_as_transcript")}
              >
                Same as transcript
              </Button>
              <Button
                type="button"
                variant={usingSpecificLanguage ? "default" : "outline"}
                size="sm"
                disabled={disabled || !allowsSpecificFinalLanguage}
                onClick={() =>
                  onFinalTextLanguageChange(usingSpecificLanguage ? finalTextLanguage : "en")
                }
              >
                Specific language
              </Button>
            </ButtonGroup>
            {usingSpecificLanguage ? (
              <div className="mt-3">
                <LanguageSelection
                  value={finalTextLanguage}
                  onValueChange={onFinalTextLanguageChange}
                  className="w-full md:w-64"
                />
              </div>
            ) : null}
          </FieldSet>

          <AppFormattingRulesEditor
            preset={preset}
            rules={writingSettings.app_formatting_rules}
            disabled={writingSettingsDisabled}
            aiFormattingEnabled={aiFormattingEnabled}
            onChange={(app_formatting_rules) =>
              onWritingSettingsChange({
                ...writingSettings,
                app_formatting_rules,
              })
            }
          />
        </section>

        <section aria-label="Dictionary">
          <CustomWordEditor
            customWords={writingSettings.custom_words}
            disabled={writingSettingsDisabled}
            onChange={(custom_words) =>
              onWritingSettingsChange({ ...writingSettings, custom_words })
            }
          />
        </section>

        <section aria-label="Corrections">
          <ReplacementEditor
            replacements={writingSettings.replacements}
            disabled={writingSettingsDisabled}
            onChange={(replacements) =>
              onWritingSettingsChange({ ...writingSettings, replacements })
            }
          />
        </section>

        <section aria-label="Snippets">
          <SnippetEditor
            snippets={writingSettings.snippets}
            disabled={writingSettingsDisabled}
            onChange={(snippets) => onWritingSettingsChange({ ...writingSettings, snippets })}
          />
        </section>
      </div>
    </div>
  );
}
