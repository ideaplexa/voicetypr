import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Switch } from "@/components/settings/SettingsSwitch";
import { toast } from "sonner";
import { SettingsPaneCard, SettingsPaneRow } from "@/components/settings/settings-ui";
import { TELEMETRY_COPY, WhatsShared } from "@/components/WhatsShared";
interface Status { enabled: boolean; available: boolean }
export function TelemetrySection() {
  const [status, setStatus] = useState<Status | null>(null);
  const [pending, setPending] = useState(false);
  useEffect(() => {
    let cancelled = false;
    void invoke<Status>("get_telemetry_status").then(value => { if (!cancelled) setStatus(value); }).catch(() => toast.error("Could not read privacy settings."));
    return () => { cancelled = true; };
  }, []);
  const update = async (enabled: boolean) => {
    setPending(true);
    try {
      const result = await invoke<Status>("set_telemetry_consent", { enabled });
      setStatus(result);
      toast.success(result.enabled ? "Anonymous sharing turned on." : "Anonymous sharing turned off.");
    } catch { toast.error("Could not save privacy settings."); }
    finally { setPending(false); }
  };
  return <SettingsPaneCard title="Anonymous sharing">
    <SettingsPaneRow title="Crash reports and usage numbers" description={TELEMETRY_COPY} control={
      <Switch checked={status?.enabled ?? true} disabled={pending || status === null} onCheckedChange={update} aria-label="Share anonymous crash reports and usage numbers" />
    } />
    <WhatsShared />
  </SettingsPaneCard>;
}
