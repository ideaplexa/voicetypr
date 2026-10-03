import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TELEMETRY_COPY, WhatsShared } from "@/components/WhatsShared";
import { Switch } from "@/components/settings/SettingsSwitch";
import { createLogger } from "@/lib/logger";

const log = createLogger("privacy-consent");

interface AnalyticsStatus {
  enabled: boolean;
  available: boolean;
  consent_required: boolean;
}

export function PrivacyConsentDialog() {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [diagnosticsEnabled, setDiagnosticsEnabled] = useState(true);
  const completedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    invoke<AnalyticsStatus>("get_telemetry_status")
      .then((status) => {
        if (cancelled) return;
        setDiagnosticsEnabled(status.enabled);
        setOpen(status.consent_required);
      })
      .catch((error) => {
        log.error("Failed to read privacy consent status:", error);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const defer = async () => {
    setOpen(false);
    try {
      await invoke("defer_privacy_consent_for_session");
    } catch (error) {
      log.error("Failed to pause telemetry for this session:", error);
    }
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (nextOpen || completedRef.current || saving) return;
    void defer();
  };

  const save = async () => {
    setSaving(true);
    try {
      // Save diagnostics first; analytics consent and its acknowledgement are
      // persisted atomically by the second command.
      await invoke("set_telemetry_consent", { enabled: diagnosticsEnabled });
      completedRef.current = true;
      setOpen(false);
    } catch (error) {
      log.error("Failed to save privacy choices:", error);
      toast.error("Could not save privacy choices. Please try again.");
    } finally {
      setSaving(false);
    }
  };

  if (loading) return null;

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Help improve Voicetypr</DialogTitle>
          <DialogDescription>
            Choose whether to share anonymous information. This choice can be changed
            anytime in Settings.
          </DialogDescription>
        </DialogHeader>

        <div className="flex items-start gap-4">
          <p className="text-sm">{TELEMETRY_COPY}</p>
          <Switch checked={diagnosticsEnabled} disabled={saving} onCheckedChange={setDiagnosticsEnabled}
            aria-label="Share anonymous crash reports and usage numbers" />
        </div>
        <WhatsShared />

        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => void defer()} disabled={saving}>
            Not now
          </Button>
          <Button type="button" onClick={() => void save()} disabled={saving}>
            {saving ? "Saving…" : "Continue"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
