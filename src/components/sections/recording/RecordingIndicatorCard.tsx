import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from "@/components/ui/field";
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
    <FieldSet className="gap-4 rounded-xl border border-border bg-card p-4">
      <FieldLegend className="mb-1 text-base font-semibold">Recording indicator</FieldLegend>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldTitle>Indicator visibility</FieldTitle>
          <FieldDescription>Show or hide the small recording status overlay.</FieldDescription>
        </FieldContent>
        <Select
          items={[
            { value: "never", label: "Never" },
            { value: "always", label: "Always" },
            { value: "when_recording", label: "When Recording" },
          ]}
          value={settings.pill_indicator_mode ?? "when_recording"}
          onValueChange={async (value) => {
            if (value == null) return;
            await updateSettings({
              pill_indicator_mode: value as PillIndicatorMode,
            });
          }}
        >
          <SelectTrigger className="w-full md:w-[190px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="never">Never</SelectItem>
            <SelectItem value="always">Always</SelectItem>
            <SelectItem value="when_recording">When Recording</SelectItem>
          </SelectContent>
        </Select>
      </Field>

      {settings.pill_indicator_mode !== "never" && (
        <Field orientation="responsive" className="items-center gap-3">
          <FieldContent>
            <FieldTitle>Indicator detail</FieldTitle>
            <FieldDescription>
              Compact shows only the animation. Full adds status text and a timer.
            </FieldDescription>
          </FieldContent>
          <Select
            items={[
              { value: "compact", label: "Compact" },
              { value: "full", label: "Full" },
            ]}
            value={settings.pill_indicator_style ?? "compact"}
            onValueChange={async (value) => {
              if (value == null) return;
              await updateSettings({
                pill_indicator_style: value as PillIndicatorStyle,
              });
            }}
          >
            <SelectTrigger className="w-full md:w-[190px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="compact">Compact</SelectItem>
              <SelectItem value="full">Full</SelectItem>
            </SelectContent>
          </Select>
        </Field>
      )}

      {settings.pill_indicator_mode !== "never" && (
        <>
          <Field orientation="responsive" className="items-center gap-3">
            <FieldContent>
              <FieldTitle>Indicator Position</FieldTitle>
              <FieldDescription>
                Choose where the status overlay appears on screen.
              </FieldDescription>
            </FieldContent>
            <Select
              items={[
                { value: "top-left", label: "Top Left" },
                { value: "top-center", label: "Top Center" },
                { value: "top-right", label: "Top Right" },
                { value: "bottom-left", label: "Bottom Left" },
                { value: "bottom-center", label: "Bottom Center" },
                { value: "bottom-right", label: "Bottom Right" },
              ]}
              value={settings.pill_indicator_position ?? "bottom-center"}
              onValueChange={async (value) => {
                if (value == null) return;
                await updateSettings({
                  pill_indicator_position: value as PillIndicatorPosition,
                });
              }}
            >
              <SelectTrigger className="w-full md:w-[190px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="top-left">Top Left</SelectItem>
                <SelectItem value="top-center">Top Center</SelectItem>
                <SelectItem value="top-right">Top Right</SelectItem>
                <SelectItem value="bottom-left">Bottom Left</SelectItem>
                <SelectItem value="bottom-center">Bottom Center</SelectItem>
                <SelectItem value="bottom-right">Bottom Right</SelectItem>
              </SelectContent>
            </Select>
          </Field>

          <Field orientation="responsive" className="items-center gap-3">
            <FieldContent>
              <FieldTitle>Edge offset</FieldTitle>
              <FieldDescription>Distance from screen edge.</FieldDescription>
            </FieldContent>
            <div className="w-full min-w-0 md:flex-1">
              <div className="flex items-center gap-3">
                <Slider
                  aria-label="Indicator edge offset"
                  min={10}
                  max={50}
                  step={5}
                  value={[settings.pill_indicator_offset ?? 10]}
                  onValueChange={async (value) => {
                    const offset = Array.isArray(value) ? value[0] : value;
                    await updateSettings({
                      pill_indicator_offset: offset,
                    });
                  }}
                  className="w-full"
                />
                <div className="min-w-12 rounded-md border bg-muted/60 px-2 py-1 text-center text-[11px] font-medium text-foreground tabular-nums">
                  {settings.pill_indicator_offset ?? 10}px
                </div>
              </div>
            </div>
          </Field>
        </>
      )}
    </FieldSet>
  );
}
