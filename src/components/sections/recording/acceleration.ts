import type { AccelerationStatus } from "@/types/acceleration";

export function isAccelerationStatus(value: unknown): value is AccelerationStatus {
  if (!value || typeof value !== "object") {
    return false;
  }

  const status = value as Record<string, unknown>;
  return (
    typeof status.message === "string" &&
    typeof status.effective_backend === "string" &&
    typeof status.diagnostic_code === "string" &&
    typeof status.recommended_action === "string"
  );
}

export function isMetalOnUnsupportedPlatformStatus(status: AccelerationStatus): boolean {
  return status.diagnostic_code === "unsupported_platform" && status.effective_backend === "metal";
}

export function getRecommendedActionDescription(action: string): string | undefined {
  switch (action) {
    case "download_model":
      return "Download or select a local Whisper model, then retry Test GPU.";
    case "update_graphics_driver":
      return "Update or install your graphics driver, then retry Test GPU.";
    case "use_cpu":
      return "Use CPU mode for now, or retry GPU after updating your graphics driver.";
    case "report_bug":
      return "Report this with logs so we can inspect the packaged GPU helper.";
    default:
      return undefined;
  }
}

export function getAccelerationGuidance(status: AccelerationStatus | null): string {
  if (!status) {
    return "Voicetypr will test GPU acceleration when needed and keep CPU transcription available.";
  }

  if (isMetalOnUnsupportedPlatformStatus(status)) {
    return "Metal acceleration is active on this Mac.";
  }

  switch (status.diagnostic_code) {
    case "ready":
      return "GPU acceleration is ready.";
    case "unsupported_platform":
      return "GPU acceleration is not available on this platform. Voicetypr will keep using CPU transcription safely.";
    case "vulkan_loader_missing":
    case "vulkan_device_missing":
    case "driver_or_runtime_failed":
      return "GPU acceleration needs a Vulkan-capable NVIDIA, AMD, or Intel graphics driver. Update or install your graphics driver, then retry Test GPU. Voicetypr will keep using CPU transcription safely until GPU acceleration is available.";
    case "sidecar_missing":
    case "sidecar_protocol_error":
      return "Voicetypr has a package or runtime issue. Please report this with logs. Voicetypr will keep using CPU transcription safely.";
    case "sidecar_timeout":
      return "The Vulkan helper did not respond in time. Voicetypr will keep using CPU transcription safely; retry Test GPU after updating your graphics driver.";
    case "model_missing":
      return "Download or select a local Whisper model before testing GPU acceleration. Voicetypr will keep using CPU transcription safely.";
    default:
      return (
        getRecommendedActionDescription(status.recommended_action) ||
        "Voicetypr will keep using CPU transcription safely."
      );
  }
}

export function getAccelerationToastDescription(status: AccelerationStatus): string | undefined {
  return (
    getRecommendedActionDescription(status.recommended_action) || status.last_error || undefined
  );
}
