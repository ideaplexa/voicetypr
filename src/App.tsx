import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AppErrorBoundary } from "./components/ErrorBoundary";
import { AppContainer } from "./components/AppContainer";
import { LicenseProvider } from "./contexts/LicenseContext";
import { ReadinessProvider } from "./contexts/ReadinessContext";
import { SettingsProvider } from "./contexts/SettingsContext";
import { ModelManagementProvider } from "./contexts/ModelManagementContext";
import { ModelAvailabilityProvider } from "./contexts/ModelAvailabilityContext";
import { useTheme } from "@/hooks/useTheme";

/** Applies the stored theme to the document root. Must live inside SettingsProvider. */
function ThemeSync() {
  useTheme();
  return null;
}

export default function App() {
  return (
    <AppErrorBoundary>
      <LicenseProvider>
        <SettingsProvider>
          <ThemeSync />
          <ModelAvailabilityProvider>
            <ReadinessProvider>
              <ModelManagementProvider>
                <TooltipProvider>
                  <AppContainer />
                  <Toaster
                    position="top-center"
                    closeButton
                    expand
                    visibleToasts={4}
                    toastOptions={{
                      duration: 5_000,
                      classNames: {
                        toast:
                          "border-border bg-popover text-popover-foreground shadow-2xl shadow-black/20 ring-1 ring-black/5",
                        title: "text-sm font-semibold",
                        description: "text-sm leading-relaxed text-muted-foreground",
                        closeButton:
                          "border-border bg-background text-muted-foreground shadow-sm hover:bg-accent hover:text-accent-foreground",
                        success: "border-sage/50",
                        error: "border-destructive/50",
                        warning: "border-amber-500/50",
                        info: "border-sky-500/50",
                      },
                    }}
                  />
                </TooltipProvider>
              </ModelManagementProvider>
            </ReadinessProvider>
          </ModelAvailabilityProvider>
        </SettingsProvider>
      </LicenseProvider>
    </AppErrorBoundary>
  );
}
