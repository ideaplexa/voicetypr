import { useRef, useState, useEffect } from "react";
import { AlertTriangle, X, Copy, Check, Send } from "lucide-react";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { CrashReportData, gatherCrashReportData, submitCrashReport } from "@/utils/crashReport";
import { getModelDisplayName } from "@/lib/model-display";
import { createLogger } from "@/lib/logger";

const log = createLogger("crash-report");

interface CrashReportDialogProps {
  error: Error;
  componentStack?: string;
  currentModel?: string | null;
  isOpen: boolean;
  onClose: () => void;
  onRetry?: () => void;
}

function buildCopyDetails(data: CrashReportData): string {
  const parts = [
    `Error: ${data.errorMessage}`,
    `Stack: ${data.errorStack || "N/A"}`,
    `App Version: ${data.appVersion}`,
    `Platform: ${data.platform} ${data.osVersion}`,
    `Architecture: ${data.architecture}`,
    `Model: ${getModelDisplayName(data.currentModel) || "None"}`,
    `Timestamp: ${data.timestamp}`,
  ];

  if (!data.systemSpecs && data.systemSpecsError) {
    parts.push("", `System Specs: unavailable (${data.systemSpecsError})`);
  }

  if (data.logContent) {
    parts.push("", "Latest App Log:", data.logContent);
  } else if (data.logStatusNote) {
    parts.push("", `Latest App Log: ${data.logStatusNote}`);
  }

  if (data.debugRingContent) {
    parts.push("", "Debug Ring (most recent, in-memory):", data.debugRingContent);
  }

  return parts.join("\n");
}

export function CrashReportDialog({
  error,
  componentStack,
  currentModel,
  isOpen,
  onClose,
  onRetry,
}: CrashReportDialogProps) {
  const [crashData, setCrashData] = useState<CrashReportData | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState("");
  const [copied, setCopied] = useState(false);
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const actionIdRef = useRef(0);

  const clearCopyTimer = () => {
    if (copiedTimerRef.current) {
      clearTimeout(copiedTimerRef.current);
      copiedTimerRef.current = null;
    }
  };

  // Render-safe session reset: state only. Refs must never be touched during
  // render; a pending copy timer firing after a reset is a harmless no-op
  // (it only sets copied back to false).
  const resetSessionState = () => {
    setCrashData(null);
    setIsLoading(true);
    setIsSubmitting(false);
    setSubmitError("");
    setCopied(false);
  };

  const resetState = () => {
    resetSessionState();
    clearCopyTimer();
  };

  // Closing invalidates any in-flight gather/submit action. Reopening increments again in the effect below.
  // The double increment is intentional: stale async completions must never update a fresh dialog session.
  const handleClose = () => {
    actionIdRef.current += 1;
    resetState();
    onClose();
  };

  // Track the current crash session; when it changes (dialog reopened or a
  // different error payload), reset per-session state during render rather
  // than adjusting state from inside an effect.
  const [prevSession, setPrevSession] = useState({
    isOpen,
    error,
    componentStack,
    currentModel,
  });
  if (
    isOpen !== prevSession.isOpen ||
    error !== prevSession.error ||
    componentStack !== prevSession.componentStack ||
    currentModel !== prevSession.currentModel
  ) {
    setPrevSession({ isOpen, error, componentStack, currentModel });
    resetSessionState();
  }

  useEffect(() => {
    return () => clearCopyTimer();
  }, []);

  useEffect(() => {
    if (!isOpen || !error) return;

    const actionId = actionIdRef.current + 1;
    actionIdRef.current = actionId;

    gatherCrashReportData(error, componentStack, currentModel)
      .then((data) => {
        if (actionId === actionIdRef.current) {
          setCrashData(data);
        }
      })
      .catch((err) => {
        if (actionId === actionIdRef.current) {
          log.error(err);
        }
      })
      .finally(() => {
        if (actionId === actionIdRef.current) {
          setIsLoading(false);
        }
      });
  }, [isOpen, error, componentStack, currentModel]);

  const handleSubmitReport = async () => {
    if (!crashData) return;

    setSubmitError("");
    setCopied(false);
    clearCopyTimer();
    const actionId = actionIdRef.current + 1;
    actionIdRef.current = actionId;
    setIsSubmitting(true);
    try {
      const result = await submitCrashReport(crashData);
      if (actionId !== actionIdRef.current) return;

      if (result.success) {
        toast.success("Crash report submitted. Thank you.");
        handleClose();
        return;
      }

      setSubmitError(
        result.message ||
          "Failed to submit crash report. You can copy the details and send them manually.",
      );
      toast.error(
        result.message || "Failed to submit crash report. Please copy the details instead.",
      );
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCopyDetails = async () => {
    if (!crashData) return;

    const details = buildCopyDetails(crashData);

    try {
      await navigator.clipboard.writeText(details);
      setCopied(true);
      toast.success("Details copied to clipboard");
      clearCopyTimer();
      copiedTimerRef.current = setTimeout(() => {
        setCopied(false);
        copiedTimerRef.current = null;
      }, 2000);
    } catch (err) {
      log.error("Failed to copy:", err);
      toast.error("Failed to copy details");
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && handleClose()}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-destructive">
            <AlertTriangle className="h-5 w-5" />
            Something went wrong
          </DialogTitle>
          <DialogDescription>
            Voicetypr hit an unexpected error. Submit a crash report with system info and the latest
            app log so we can fix it.
          </DialogDescription>
        </DialogHeader>

        {isLoading ? (
          <div className="py-8 text-center text-muted-foreground">Gathering crash details...</div>
        ) : crashData ? (
          <div className="space-y-4">
            {/* Error Message */}
            <div className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Error
              </p>
              <div className="rounded-md bg-destructive/10 border border-destructive/20 p-3">
                <p className="text-sm font-mono text-destructive break-all">
                  {crashData.errorMessage}
                </p>
              </div>
            </div>

            {/* Stack Trace (collapsible) */}
            {crashData.errorStack && (
              <div className="space-y-2">
                <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  Stack Trace
                </p>
                <ScrollArea className="h-32 rounded-md border bg-muted/50 p-3">
                  <pre className="text-xs font-mono text-muted-foreground whitespace-pre-wrap break-all">
                    {crashData.errorStack}
                  </pre>
                </ScrollArea>
              </div>
            )}

            {/* System Info */}
            <div className="space-y-2">
              <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                System Info
              </p>
              <div className="grid grid-cols-2 gap-2 text-xs">
                <div className="flex justify-between p-2 rounded bg-muted/50">
                  <span className="text-muted-foreground">Version</span>
                  <span className="font-medium">{crashData.appVersion}</span>
                </div>
                <div className="flex justify-between p-2 rounded bg-muted/50">
                  <span className="text-muted-foreground">Platform</span>
                  <span className="font-medium">{crashData.platform}</span>
                </div>
                <div className="flex justify-between p-2 rounded bg-muted/50">
                  <span className="text-muted-foreground">OS</span>
                  <span className="font-medium">{crashData.osVersion}</span>
                </div>
                <div className="flex justify-between p-2 rounded bg-muted/50">
                  <span className="text-muted-foreground">Model</span>
                  <span className="font-medium">
                    {getModelDisplayName(crashData.currentModel) || "None"}
                  </span>
                </div>
              </div>
            </div>

            {submitError && (
              <div
                className="rounded-md border border-destructive/30 bg-destructive/10 p-3"
                role="alert"
              >
                <p className="text-xs text-destructive">{submitError}</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  If this keeps happening, copy the crash details and send them manually.
                </p>
              </div>
            )}
          </div>
        ) : (
          <div className="py-8 text-center text-muted-foreground">
            Failed to gather crash details
          </div>
        )}

        <DialogFooter className="flex-col sm:flex-row gap-2">
          {submitError && (
            <Button
              variant="outline"
              size="sm"
              onClick={handleCopyDetails}
              disabled={!crashData || isSubmitting}
              className="gap-2"
            >
              {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
              {copied ? "Copied" : "Copy Details"}
            </Button>
          )}

          <div className="flex gap-2 sm:ml-auto">
            {onRetry && (
              <Button variant="outline" size="sm" onClick={onRetry} disabled={isSubmitting}>
                Try Again
              </Button>
            )}
            <Button variant="ghost" size="sm" onClick={handleClose}>
              <X className="h-4 w-4 mr-1" />
              Dismiss
            </Button>
            <Button
              size="sm"
              onClick={handleSubmitReport}
              disabled={!crashData || isLoading || isSubmitting}
              aria-busy={isSubmitting}
              className="gap-2"
            >
              <Send className="h-4 w-4" />
              {isSubmitting ? "Submitting..." : "Submit"}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
