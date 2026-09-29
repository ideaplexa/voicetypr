import { SettingsCard, SettingRow } from "@/components/settings/settings-ui";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import { useSettings } from "@/contexts/SettingsContext";
import type { PillIndicatorMode, PillIndicatorPosition, PillIndicatorStyle } from "@/types";

export function RecordingIndicatorCard() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;
  return (
    <SettingsCard title="Recording pill" className="[&>div:first-child]:sr-only [&>div:last-child]:mt-0">
      <div className="grid items-center gap-[18px] sm:grid-cols-[300px_minmax(0,1fr)]">
        <div className="flex h-[120px] items-center justify-center rounded-[10px] bg-muted p-4">
          <div className="flex items-center gap-2 rounded-full border border-white/10 bg-[#121316] px-3 py-2 text-white shadow-xl">
            <div className="flex h-5 items-center gap-0.5">
              {[7, 13, 19, 11, 17, 9, 14, 6, 12].map((height, index) => (
                <span
                  key={index}
                  className="w-[2px] rounded-full bg-[#8FD1A8]"
                  style={{ height }}
                />
              ))}
            </div>
            <span className="text-[11px]">Listening</span>
            <span className="font-mono text-[10px] text-white/60">0:07</span>
          </div>
        </div>
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
                value={settings.pill_indicator_mode ?? "when_recording"}
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
                        .map((part) => part[0].toUpperCase() + part.slice(1))
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
                            .map((part) => part[0].toUpperCase() + part.slice(1))
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
