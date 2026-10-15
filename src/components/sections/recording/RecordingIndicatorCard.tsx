import { IslandPreview } from "@/components/sections/recording/IslandPreview";
import { Segmented, SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/settings/SettingsSwitch";
import { useRealtimeWords } from "@/components/sections/models/useRealtimeWords";
import { islandLiveWordsActive } from "@/components/sections/models/modelOptionRules";
import { useSettings } from "@/contexts/SettingsContext";
import { emit } from "@tauri-apps/api/event";
import { DEFAULT_PILL_INDICATOR_MODE, type PillIndicatorMode, type PillIndicatorPosition, type PillIndicatorStyle, type AppSettings } from "@/types";

type StartDetails = NonNullable<AppSettings["island_start_details"]>;
// Stored values stay never / changed / always; the labels say what the island does.
const START_DETAILS: Array<{ value: StartDetails; label: string; description: string }> = [
  { value: "never", label: "Compact", description: "Always the slim island. Never expands." },
  { value: "changed", label: "Smart", description: "Expands briefly only when something changed (app style, mic or engine)." },
  { value: "always", label: "Expanded", description: "Always shows app, style, mic and engine when you start." },
];

export function RecordingIndicatorCard() {
  const { settings, updateSettings } = useSettings();
  const realtime = useRealtimeWords(
    settings?.current_model_engine ?? "whisper",
    settings?.current_model ?? "",
  );
  if (!settings) return null;
  const liveWordsAvailable = islandLiveWordsActive(realtime.streams, realtime.realtimeOn);
  const startDetails: StartDetails = settings.island_start_details ?? "changed";
  return (
    <SettingsCard
      title="Recording pill"
      className="[&>div:first-child]:sr-only [&>div:last-child]:mt-4"
    >
      <div className="grid items-center gap-[18px] sm:grid-cols-[300px_minmax(0,1fr)]">
        <IslandPreview position={settings.pill_indicator_position ?? "bottom-center"} />
        <div className="flex flex-col gap-2.5 [&>div]:py-0 [&>div]:border-0">
          <SettingRow
            title="Show"
            control={
              <Select
                items={[
                  { value: "never", label: "Never" },
                  { value: "always", label: "Always" },
                  { value: "when_recording", label: "While recording" },
                ]}
                value={settings.pill_indicator_mode ?? DEFAULT_PILL_INDICATOR_MODE}
                onValueChange={(value) => {
                  if (value != null)
                    void updateSettings({ pill_indicator_mode: value as PillIndicatorMode });
                }}
              >
                <SelectTrigger aria-label="Pill visibility" className="w-40">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="never">Never</SelectItem>
                  <SelectItem value="always">Always</SelectItem>
                  <SelectItem value="when_recording">While recording</SelectItem>
                </SelectContent>
              </Select>
            }
          />
          {settings.pill_indicator_mode !== "never" ? (
            <>
              <SettingRow
                title="Position"
                control={
                  <Select
                    items={[
                      "top-left",
                      "top-center",
                      "top-right",
                      "bottom-left",
                      "bottom-center",
                      "bottom-right",
                    ].map((value) => ({
                      value,
                      label: value
                        .split("-")
                        .map((part, index) =>
                          index === 0 ? part[0].toUpperCase() + part.slice(1) : part,
                        )
                        .join(" "),
                    }))}
                    value={settings.pill_indicator_position ?? "bottom-center"}
                    onValueChange={(value) => {
                      if (value != null)
                        void updateSettings({
                          pill_indicator_position: value as PillIndicatorPosition,
                        });
                    }}
                  >
                    <SelectTrigger aria-label="Pill position" className="w-40">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {[
                        "top-left",
                        "top-center",
                        "top-right",
                        "bottom-left",
                        "bottom-center",
                        "bottom-right",
                      ].map((value) => (
                        <SelectItem key={value} value={value}>
                          {value
                            .split("-")
                            .map((part, index) =>
                              index === 0 ? part[0].toUpperCase() + part.slice(1) : part,
                            )
                            .join(" ")}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                }
              />
              <SettingRow
                title="Detail"
                control={
                  <Select
                    items={[
                      { value: "compact", label: "Level only" },
                      { value: "full", label: "Level + timer" },
                    ]}
                    value={settings.pill_indicator_style ?? "compact"}
                    onValueChange={(value) => {
                      if (value != null)
                        void updateSettings({ pill_indicator_style: value as PillIndicatorStyle });
                    }}
                  >
                    <SelectTrigger aria-label="Pill detail" className="w-40">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="compact">Level only</SelectItem>
                      <SelectItem value="full">Level + timer</SelectItem>
                    </SelectContent>
                  </Select>
                }
              />
            </>
          ) : null}
        </div>
      </div>
      {settings.pill_indicator_mode !== "never" ? (
        <SettingRow
          title="Start details"
          description={START_DETAILS.find((option) => option.value === startDetails)?.description}
          control={
            <Segmented
              label="Start details"
              value={startDetails}
              onValueChange={(value) => void updateSettings({ island_start_details: value as StartDetails })}
              options={START_DETAILS}
            />
          }
        />
      ) : null}
      {settings.pill_indicator_mode !== "never" ? (
        <SettingRow
          title="Show live words while you talk"
          description={
            liveWordsAvailable ? (
              "The island grows to show words as they come in."
            ) : (
              <>
                <button
                  type="button"
                  className="underline underline-offset-2"
                  onClick={() => void emit("navigate-to-section", "transcription")}
                >
                  Turn on Real-time words in Transcription
                </button>
              </>
            )
          }
          control={
            <Switch
              aria-label="Show live words while you talk"
              checked={liveWordsAvailable && settings.island_live_words !== false}
              disabled={!liveWordsAvailable}
              onCheckedChange={(checked) => void updateSettings({ island_live_words: checked })}
            />
          }
        />
      ) : null}
      {settings.pill_indicator_mode !== "never" ? (
        <SettingRow
          title="Edge offset"
          description="Distance from screen edge."
          control={
            <div className="flex w-44 items-center gap-2">
              <Slider
                aria-label="Indicator edge offset"
                min={10}
                max={50}
                step={5}
                value={[settings.pill_indicator_offset ?? 10]}
                onValueChange={(value) => {
                  const offset = Array.isArray(value) ? value[0] : value;
                  void updateSettings({ pill_indicator_offset: offset });
                }}
              />
              <span className="text-xs tabular-nums text-muted-foreground">
                {settings.pill_indicator_offset ?? 10}px
              </span>
            </div>
          }
        />
      ) : null}
    </SettingsCard>
  );
}
