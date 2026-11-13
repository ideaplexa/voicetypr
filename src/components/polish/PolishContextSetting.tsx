import { useState } from "react";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { SettingRow } from "@/components/settings/settings-ui";
import { Switch } from "@/components/settings/SettingsSwitch";

type CaptureStatus = "available" | "partial" | "disabled" | "secure" | "denied"
  | "unsupported" | "timeout" | "target_changed";
type CaptureUpdate = { generation: number; status: CaptureStatus };
const statusLabels: Record<CaptureStatus, string> = {
  available: "Nearby text is available for Polish.",
  partial: "Partial nearby text is available for Polish.",
  disabled: "Not captured: Polish or nearby text is off.",
  secure: "Not captured: protected field or app.",
  denied: "Not captured: access is unavailable.",
  unsupported: "Not captured: this target is not supported.",
  timeout: "Not captured: capture timed out.",
  target_changed: "Not captured: the target changed.",
};

export function PolishContextSetting({ checked, onCheckedChange }: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  const [latest, setLatest] = useState<CaptureUpdate | null>(null);
  useTauriEvent<CaptureUpdate>("polish-context-status", (update) => {
    setLatest((previous) => previous && previous.generation > update.generation ? previous : update);
  });
  return <SettingRow title="Use nearby text" htmlFor="polish-context-enabled"
    description={<>
      When Polish is on, text before your cursor and your selection are sent to your chosen Polish provider, including cloud providers. Voicetypr does not save this context.
      {checked && latest && <span className="mt-1 block" role="status">{statusLabels[latest.status]}</span>}
    </>}
    control={<Switch id="polish-context-enabled" checked={checked} onCheckedChange={onCheckedChange} />} />;
}
