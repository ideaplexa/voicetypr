import { createContext, useContext, useMemo, ReactNode } from "react";
import { useAppReadiness } from "@/hooks/useAppReadiness";

type ReadinessContextType = ReturnType<typeof useAppReadiness>;

const ReadinessContext = createContext<ReadinessContextType | null>(null);

export function ReadinessProvider({ children }: { children: ReactNode }) {
  const r = useAppReadiness();
  // useAppReadiness returns a fresh object each render; keep the provided value
  // referentially stable until a field actually changes.
  const readinessState = useMemo(
    () => r,
    // eslint-disable-next-line react-hooks/exhaustive-deps -- fields listed explicitly below
    [
      r.hasAccessibilityPermission,
      r.hasMicrophonePermission,
      r.hasModels,
      r.selectedModelAvailable,
      r.licenseValid,
      r.licenseStatus,
      r.remoteSelected,
      r.remoteAvailable,
      r.remoteStatus,
      r.canRecord,
      r.canAutoInsert,
      r.isFullyReady,
      r.isLoading,
      r.requestAccessibilityPermission,
      r.requestMicrophonePermission,
      r.checkAccessibilityPermission,
      r.checkMicrophonePermission,
      r.checkModels,
      r.checkLicense,
    ],
  );

  return <ReadinessContext.Provider value={readinessState}>{children}</ReadinessContext.Provider>;
}

export function useReadiness() {
  const context = useContext(ReadinessContext);
  if (!context) {
    throw new Error("useReadiness must be used within a ReadinessProvider");
  }
  return context;
}

// Convenience helpers that match the computed values from useAppReadiness
export function useCanRecord() {
  const context = useContext(ReadinessContext);
  return context?.canRecord || false;
}

export function useCanAutoInsert() {
  const context = useContext(ReadinessContext);
  return context?.canAutoInsert || false;
}

// Helper to get the full readiness state (for components that need access to everything)
export function useReadinessState() {
  const context = useContext(ReadinessContext);
  if (!context) return null;

  // Return a simplified object that matches the old AppReadiness interface
  // This helps with backward compatibility during the refactor
  return {
    has_accessibility_permission: context.hasAccessibilityPermission,
    has_microphone_permission: context.hasMicrophonePermission,
    has_models: context.hasModels,
    selected_model_available: context.selectedModelAvailable,
    license_valid: context.licenseValid,
    ai_ready: false, // This would need to be computed based on AI settings
  };
}
