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
import { Switch } from "@/components/ui/switch";
import { createLogger } from "@/lib/logger";

const log = createLogger("privacy-consent");

interface DiagnosticsStatus {
  enabled: boolean;
  available: boolean;
}

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
  const [analyticsEnabled, setAnalyticsEnabled] = useState(true);
  const completedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      invoke<DiagnosticsStatus>("get_telemetry_status"),
      invoke<AnalyticsStatus>("get_product_analytics_status"),
    ])
      .then(([diagnostics, analytics]) => {
        if (cancelled) return;
        setDiagnosticsEnabled(diagnostics.enabled);
        setAnalyticsEnabled(analytics.enabled);
        setOpen(analytics.consent_required);
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
      await invoke("set_product_analytics_consent", { enabled: analyticsEnabled });
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
            Choose what anonymous information Voicetypr may send. Both options can be changed
            anytime in Settings.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <div className="flex items-start justify-between gap-4 rounded-2xl border border-border bg-card p-4">
            <div className="min-w-0">
              <p className="font-medium">Crash &amp; error reporting</p>
              <p className="mt-1 text-xs text-muted-foreground">
                Sends scrubbed crash and error details to GlitchTip so bugs can be diagnosed.
              </p>
            </div>
            <Switch
              checked={diagnosticsEnabled}
              disabled={saving}
              onCheckedChange={setDiagnosticsEnabled}
              aria-label="Enable crash and error reporting"
              className="shrink-0"
            />
          </div>

          <div className="flex items-start justify-between gap-4 rounded-2xl border border-border bg-card p-4">
            <div className="min-w-0">
              <p className="font-medium">Usage analytics</p>
              <p className="mt-1 text-xs text-muted-foreground">
                Sends anonymous feature usage, outcome, and performance buckets to PostHog.
              </p>
            </div>
            <Switch
              checked={analyticsEnabled}
              disabled={saving}
              onCheckedChange={setAnalyticsEnabled}
              aria-label="Enable usage analytics"
              className="shrink-0"
            />
          </div>
        </div>

        <p className="text-xs text-muted-foreground">
          Never includes audio, transcripts, clipboard contents, prompts, API keys, file paths,
          window titles, or session replay.
        </p>

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
