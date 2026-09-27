import { createContext, useContext, ReactNode } from "react";
import { useModelManagement } from "@/hooks/useModelManagement";

type ModelManagementContextType = ReturnType<typeof useModelManagement>;

const ModelManagementContext = createContext<ModelManagementContextType | null>(null);

export function ModelManagementProvider({ children }: { children: ReactNode }) {
  const modelManagement = useModelManagement({
    windowId: "main",
    showToasts: true,
  });

  return (
    <ModelManagementContext.Provider value={modelManagement}>
      {children}
    </ModelManagementContext.Provider>
  );
}

export function useModelManagementContext() {
  const context = useContext(ModelManagementContext);
  if (!context) {
    throw new Error("useModelManagementContext must be used within ModelManagementProvider");
  }
  return context;
}
