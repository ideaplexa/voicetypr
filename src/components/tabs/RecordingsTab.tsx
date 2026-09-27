import { useSettings } from "@/contexts/SettingsContext";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { RecentRecordings } from "../sections/RecentRecordings";

export function RecordingsTab() {
  const { settings } = useSettings();
  // Load the full history (generous cap covering any realistic local store); the
  // list itself is paginated client-side in RecentRecordings so rendering stays fast.
  const { history, refreshHistory, isLoading, loadError } = useTranscriptionHistory({
    limit: 10000,
  });
  // Resolve the ACTIVE primary trigger instead of assuming a combo hotkey. A
  // bare-modifier primary intentionally leaves `settings.hotkey` empty (the real
  // trigger lives in ShortcutSettings), so `kbdLabel` falls back to the modifier
  // key token — never the stale "Cmd+Shift+Space" default.
  const { kbdLabel } = useActiveTrigger(settings?.hotkey);

  return (
    <RecentRecordings
      history={history}
      hotkey={kbdLabel}
      onHistoryUpdate={refreshHistory}
      isLoading={isLoading}
      loadError={loadError}
    />
  );
}
