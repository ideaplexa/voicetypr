import { AlertCircle, RefreshCw, Bug } from "lucide-react";
import React, { useState } from "react";
import { ErrorBoundary as ReactErrorBoundary, type FallbackProps } from "react-error-boundary";
import { Button } from "./ui/button";
import { CrashReportDialog } from "./CrashReportDialog";
import { createLogger } from "@/lib/logger";

const log = createLogger("error-boundary");

type ErrorFallbackProps = FallbackProps;

const asError = (error: unknown): Error => {
  if (error instanceof Error) {
    return error;
  }
  if (typeof error === "string" && error.length > 0) {
    return new Error(error);
  }
  return new Error("Unknown error");
};

function ErrorFallback({ error, resetErrorBoundary }: ErrorFallbackProps) {
  const [showReportDialog, setShowReportDialog] = useState(false);

  return (
    <>
      <div className="flex flex-col items-center justify-center min-h-[200px] p-8 space-y-4">
        <AlertCircle className="w-12 h-12 text-destructive" />
        <div className="text-center space-y-2">
          <h2 className="text-lg font-semibold">Something went wrong</h2>
          <p className="text-sm text-muted-foreground max-w-md">
            {asError(error).message || "An unexpected error occurred"}
          </p>
        </div>
        <div className="flex gap-2">
          <Button onClick={resetErrorBoundary} variant="outline" size="sm" className="gap-2">
            <RefreshCw className="w-4 h-4" />
            Try again
          </Button>
          <Button
            onClick={() => setShowReportDialog(true)}
            variant="destructive"
            size="sm"
            className="gap-2"
          >
            <Bug className="w-4 h-4" />
            Report Bug
          </Button>
        </div>
      </div>

      {showReportDialog && (
        <CrashReportDialog
          error={asError(error)}
          isOpen={showReportDialog}
          onClose={() => setShowReportDialog(false)}
          onRetry={resetErrorBoundary}
        />
      )}
    </>
  );
}

interface AppErrorBoundaryProps {
  children: React.ReactNode;
  fallback?: React.ComponentType<FallbackProps>;
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
  onReset?: () => void;
  showCrashReporter?: boolean;
}

export function AppErrorBoundary({
  children,
  fallback,
  onError,
  onReset,
  showCrashReporter = true,
}: AppErrorBoundaryProps) {
  const FallbackComponent = fallback || (showCrashReporter ? ErrorFallback : SimpleFallback);

  return (
    <ReactErrorBoundary
      FallbackComponent={FallbackComponent}
      onError={(error, errorInfo) => {
        const normalizedError = asError(error);
        log.error("Error caught by boundary:", normalizedError, errorInfo);
        if (onError) {
          onError(normalizedError, errorInfo);
        }
      }}
      onReset={onReset}
    >
      {children}
    </ReactErrorBoundary>
  );
}

// Simple fallback without crash reporter (for nested boundaries)
function SimpleFallback({ error, resetErrorBoundary }: ErrorFallbackProps) {
  return (
    <div className="flex flex-col items-center justify-center min-h-[200px] p-8 space-y-4">
      <AlertCircle className="w-12 h-12 text-destructive" />
      <div className="text-center space-y-2">
        <h2 className="text-lg font-semibold">Something went wrong</h2>
        <p className="text-sm text-muted-foreground max-w-md">
          {asError(error).message || "An unexpected error occurred"}
        </p>
      </div>
      <Button onClick={resetErrorBoundary} variant="outline" size="sm" className="gap-2">
        <RefreshCw className="w-4 h-4" />
        Try again
      </Button>
    </div>
  );
}

// Specific error boundaries for different features (use simple fallback to avoid nested dialogs)
export function RecordingErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <AppErrorBoundary
      showCrashReporter={false}
      onError={(error) => {
        log.error("Recording error:", error);
      }}
      onReset={() => {
        // Could reset recording state here
      }}
    >
      {children}
    </AppErrorBoundary>
  );
}

export function SettingsErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <AppErrorBoundary
      onError={(error) => {
        log.error("Settings error:", error);
      }}
      fallback={({ error, resetErrorBoundary }) => (
        <div className="p-4 border border-destructive/20 rounded-lg bg-destructive/5">
          <p className="text-sm text-destructive">
            Failed to load settings: {asError(error).message}
          </p>
          <Button onClick={resetErrorBoundary} variant="ghost" size="sm" className="mt-2">
            Retry
          </Button>
        </div>
      )}
    >
      {children}
    </AppErrorBoundary>
  );
}

export function ModelManagementErrorBoundary({ children }: { children: React.ReactNode }) {
  return (
    <AppErrorBoundary
      onError={(error) => {
        log.error("Model management error:", error);
      }}
      fallback={({ resetErrorBoundary }) => (
        <div className="text-center p-4 space-y-2">
          <p className="text-sm text-muted-foreground">Error loading models</p>
          <Button onClick={resetErrorBoundary} size="sm" variant="outline">
            Reload Models
          </Button>
        </div>
      )}
    >
      {children}
    </AppErrorBoundary>
  );
}
