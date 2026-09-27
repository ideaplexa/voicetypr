import { createContext, useContext, ReactNode } from "react";
import { useAppReadiness } from "@/hooks/useAppReadiness";

type ReadinessContextType = ReturnType<typeof useAppReadiness>;

const ReadinessContext = createContext<ReadinessContextType | null>(null);

export function ReadinessProvider({ children }: { children: ReactNode }) {
  const readinessState = useAppReadiness();

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
