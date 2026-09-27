import { Button } from "@/components/ui/button";
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
import { useSettings } from "@/contexts/SettingsContext";
import { createLogger } from "@/lib/logger";
import { isWindows } from "@/lib/platform";
import type { AccelerationStatus } from "@/types/acceleration";
import type { TranscriptionAcceleration } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  getAccelerationGuidance,
  getAccelerationToastDescription,
  isAccelerationStatus,
} from "./acceleration";

const log = createLogger("recording-settings");

export function TranscriptionPerformanceCard() {
  const { settings, updateSettings } = useSettings();
  const [accelerationStatus, setAccelerationStatus] = useState<AccelerationStatus | null>(null);
  const [testingAcceleration, setTestingAcceleration] = useState(false);

  const loadAccelerationStatus = useCallback(async () => {
    try {
      const status = await invoke<AccelerationStatus>("get_transcription_acceleration_status");
      if (isAccelerationStatus(status)) {
        setAccelerationStatus(status);
      }
    } catch (error) {
      log.error("Failed to check acceleration status:", error);
    }
  }, []);

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      void loadAccelerationStatus();
    }, 0);
    return () => window.clearTimeout(timeoutId);
  }, [loadAccelerationStatus]);

  if (!settings || !isWindows) return null;

  const handleAccelerationChange = async (value: TranscriptionAcceleration) => {
    await updateSettings({ transcription_acceleration: value });
    await loadAccelerationStatus();
    toast.success(
      value === "auto"
        ? "Acceleration set to Auto"
        : value === "gpu"
          ? "GPU acceleration preferred"
          : "CPU-only transcription enabled",
    );
  };

  const handleTestAcceleration = async () => {
    setTestingAcceleration(true);
    try {
      const status = await invoke<AccelerationStatus>("test_transcription_acceleration");
      if (isAccelerationStatus(status)) {
        setAccelerationStatus(status);
        if (status.gpu_available === true) {
          toast.success(status.message || "GPU acceleration is available");
        } else if (status.gpu_available === false) {
          toast.warning(status.message, {
            description: getAccelerationToastDescription(status),
          });
        } else {
          toast.info(status.message);
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error("GPU acceleration test failed", { description: message });
      await loadAccelerationStatus();
    } finally {
      setTestingAcceleration(false);
    }
  };

  return (
    <FieldSet className="gap-4 rounded-xl border border-border bg-card p-4">
      <FieldLegend className="mb-1 text-base font-semibold">Transcription performance</FieldLegend>

      <Field orientation="responsive" className="items-center gap-3">
        <FieldContent>
          <FieldTitle>Acceleration</FieldTitle>
          <FieldDescription>
            {(settings.transcription_acceleration ?? "auto") === "auto"
              ? "Use GPU when available, fall back to CPU (recommended)"
              : (settings.transcription_acceleration ?? "auto") === "gpu"
                ? "Always use the GPU"
                : "Always use the CPU"}
          </FieldDescription>
        </FieldContent>
        <Select
          items={[
            { value: "auto", label: "Auto" },
            { value: "gpu", label: "GPU" },
            { value: "cpu", label: "CPU" },
          ]}
          value={settings.transcription_acceleration ?? "auto"}
          onValueChange={(value) => {
            if (value != null) void handleAccelerationChange(value);
          }}
        >
          <SelectTrigger className="w-full md:w-[190px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="auto">Auto</SelectItem>
            <SelectItem value="gpu">GPU</SelectItem>
            <SelectItem value="cpu">CPU</SelectItem>
          </SelectContent>
        </Select>
      </Field>

      <div className="flex items-start justify-between gap-4 rounded-lg bg-muted/40 p-3">
        <div className="space-y-1">
          <p className="text-sm font-medium">
            {accelerationStatus?.effective_backend === "vulkan"
              ? "GPU acceleration ready"
              : accelerationStatus?.effective_backend === "metal"
                ? "Using Metal acceleration"
                : accelerationStatus?.effective_backend === "cpu"
                  ? "Using CPU mode"
                  : "Acceleration status"}
          </p>
          <p className="text-xs text-muted-foreground">
            {accelerationStatus?.message ?? "Voicetypr will test GPU acceleration when needed."}
          </p>
          {accelerationStatus?.diagnostic_code !== "ready" && (
            <p className="text-xs text-muted-foreground">
              {getAccelerationGuidance(accelerationStatus)}
            </p>
          )}
          {accelerationStatus?.last_error && (
            <p className="text-xs text-amber-600 dark:text-amber-500">
              {accelerationStatus.last_error}
            </p>
          )}
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={handleTestAcceleration}
          disabled={testingAcceleration}
        >
          {testingAcceleration ? "Checking..." : isWindows ? "Test GPU" : "Check Status"}
        </Button>
      </div>
    </FieldSet>
  );
}
