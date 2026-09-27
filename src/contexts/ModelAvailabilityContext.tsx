import { createContext, useContext, ReactNode } from "react";
import { useModelAvailability } from "@/hooks/useModelAvailability";
import type { ModelAvailability } from "@/hooks/useModelAvailability";

const ModelAvailabilityContext = createContext<ModelAvailability | null>(null);

export function ModelAvailabilityProvider({ children }: { children: ReactNode }) {
  const modelAvailability = useModelAvailability();

  return (
    <ModelAvailabilityContext.Provider value={modelAvailability}>
      {children}
    </ModelAvailabilityContext.Provider>
  );
}

export function useModelAvailabilityContext(): ModelAvailability {
  const context = useContext(ModelAvailabilityContext);
  if (!context) {
    throw new Error("useModelAvailabilityContext must be used within ModelAvailabilityProvider");
  }
  return context;
}
