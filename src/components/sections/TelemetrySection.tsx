import { invoke } from "@tauri-apps/api/core";
import { Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Switch } from "@/components/ui/switch";
import { createLogger } from "@/lib/logger";
import { toast } from "sonner";

const log = createLogger("telemetry");

interface DiagnosticsStatus {
  enabled: boolean;
  available: boolean;
}

interface AnalyticsStatus {
  enabled: boolean;
  available: boolean;
  consent_required: boolean;
}

/// Mirrors the Rust `TelemetryConsentResult`.
interface TelemetryConsentResult {
  enabled: boolean;
  /** Opt-in only wires the Sentry client on the next launch; opt-out is immediate. */
  restart_required: boolean;
}

type PendingControl = "diagnostics" | "analytics" | null;

export function TelemetrySection() {
  const [diagnostics, setDiagnostics] = useState<DiagnosticsStatus | null>(null);
  const [analytics, setAnalytics] = useState<AnalyticsStatus | null>(null);
  const [pending, setPending] = useState<PendingControl>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      invoke<DiagnosticsStatus>("get_telemetry_status"),
      invoke<AnalyticsStatus>("get_product_analytics_status"),
    ])
      .then(([nextDiagnostics, nextAnalytics]) => {
        if (cancelled) return;
        setDiagnostics(nextDiagnostics);
        setAnalytics(nextAnalytics);
      })
      .catch((error) => {
        log.error("Failed to read privacy settings:", error);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const updateDiagnostics = async (enabled: boolean) => {
    setPending("diagnostics");
    try {
      const result = await invoke<TelemetryConsentResult>("set_telemetry_consent", {
        enabled,
      });
      setDiagnostics((current) => (current ? { ...current, enabled: result.enabled } : current));
      // Enabling mid-session cannot wire the Sentry client until the next
      // launch, so say so instead of claiming reporting is live.
      toast.success(
        result.restart_required
          ? "Crash reporting will turn on after you restart VoiceTypr."
          : enabled
            ? "Crash reporting turned on."
            : "Crash reporting turned off.",
      );
    } catch (error) {
      log.error("Failed to update crash reporting:", error);
      toast.error("Could not update crash reporting.");
    } finally {
      setPending(null);
    }
  };

  const updateAnalytics = async (enabled: boolean) => {
    setPending("analytics");
    try {
      await invoke("set_product_analytics_consent", { enabled });
      setAnalytics((current) =>
        current ? { ...current, enabled, consent_required: false } : current,
      );
      toast.success(enabled ? "Usage analytics turned on." : "Usage analytics turned off.");
    } catch (error) {
      log.error("Failed to update usage analytics:", error);
      toast.error("Could not update usage analytics.");
    } finally {
      setPending(null);
    }
  };

  const loading = diagnostics === null || analytics === null;

  return (
    <div className="flex flex-col gap-4">
      <div>
        <h2 className="text-base font-semibold">Privacy &amp; diagnostics</h2>
        <p className="mt-1 text-xs text-muted-foreground">
          Control anonymous crash reporting and product analytics independently.
        </p>
      </div>

      <div className="flex flex-col gap-3 rounded-lg border border-border/50 bg-card p-4">
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <p className="text-sm font-medium">Crash &amp; error reporting</p>
            <p className="mt-1 text-xs text-muted-foreground">
              Anonymous crash reports that help us fix bugs faster.
            </p>
          </div>
          <Switch
            className="shrink-0"
            checked={diagnostics?.enabled ?? true}
            disabled={
              pending !== null ||
              diagnostics === null ||
              (!diagnostics.available && !diagnostics.enabled)
            }
            onCheckedChange={updateDiagnostics}
            aria-label="Enable crash and error reporting"
          />
        </div>

        <div className="flex items-start justify-between gap-4 border-t border-border/50 pt-3">
          <div className="min-w-0">
            <p className="text-sm font-medium">Usage analytics</p>
            <p className="mt-1 text-xs text-muted-foreground">
              Anonymous usage analytics that help us improve the product.
            </p>
          </div>
          <Switch
            className="shrink-0"
            checked={analytics?.enabled ?? true}
            disabled={
              pending !== null || analytics === null || (!analytics.available && !analytics.enabled)
            }
            onCheckedChange={updateAnalytics}
            aria-label="Enable usage analytics"
          />
        </div>

        {loading && (
          <div className="flex items-center gap-2 text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" />
            <span className="text-xs">Checking…</span>
          </div>
        )}

        <p className="text-xs text-muted-foreground">
          No audio, transcripts, or personal data — ever.
        </p>
      </div>
    </div>
  );
}
