import { useSettings } from "@/contexts/SettingsContext";
import { useActiveTrigger } from "@/hooks/useActiveTrigger";
import { useTranscriptionHistory } from "@/hooks/useTranscriptionHistory";
import { RecentRecordings } from "../sections/RecentRecordings";
import type { ScreenId, SettingsPane } from "@/components/navigation";
import type { SourceFilter } from "@/components/sections/models/types";

export function RecordingsTab({
  onTranscribeFile,
  onNavigate,
  onNavigateSettingsPane,
  onSourceFilterChange,
}: {
  onTranscribeFile?: () => void;
  onNavigate?: (screen: ScreenId) => void;
  onNavigateSettingsPane?: (pane: SettingsPane) => void;
  onSourceFilterChange?: (source: SourceFilter) => void;
} = {}) {
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
  const { kbdLabel } = useActiveTrigger(settings);

  return (
    <RecentRecordings
      history={history}
      onTranscribeFile={onTranscribeFile}
      hotkey={kbdLabel}
      onHistoryUpdate={refreshHistory}
      isLoading={isLoading}
      loadError={loadError}
      onNavigate={onNavigate}
      onNavigateSettingsPane={onNavigateSettingsPane}
      onSourceFilterChange={onSourceFilterChange}
    />
  );
}
