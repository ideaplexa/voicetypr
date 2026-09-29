import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle, CircleCheck, Check, Copy, Keyboard, Sparkles, Wrench } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { getVersion } from "@tauri-apps/api/app";
import { UpdateAnnouncementDialog } from "@/components/UpdateAnnouncementDialog";
import type { SettingsPane } from "@/components/navigation";
import { Textarea } from "@/components/ui/textarea";
import {
  ChoiceLink,
  PageHeader,
  SettingsCard,
  SettingsPage,
} from "@/components/settings/settings-ui";
import {
  buildReportBody,
  gatherManualReportData,
  submitManualReport,
  type ManualReportData,
} from "@/utils/crashReport";
import { useSettings } from "@/contexts/SettingsContext";
import { useModelManagementContext } from "@/contexts/ModelManagementContext";
import { getModelDisplayName } from "@/lib/model-display";
import { createLogger } from "@/lib/logger";

const log = createLogger("report-problem");
export function ReportProblemSection({
  onNavigateSettingsPane,
}: {
  onNavigateSettingsPane?: (pane: SettingsPane) => void;
}) {
  const [version, setVersion] = useState<string | null>(null);
  const [announcementOpen, setAnnouncementOpen] = useState(false);
  const [sent, setSent] = useState(false);
  useEffect(() => {
    let active = true;
    void getVersion()
      .then((value) => {
        if (active) setVersion(value);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);
  const [message, setMessage] = useState("");
  const [messageError, setMessageError] = useState("");
  const [submitError, setSubmitError] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [copied, setCopied] = useState(false);
  const [fallbackReportData, setFallbackReportData] = useState<ManualReportData | null>(null);
  const { settings } = useSettings();
  const { models } = useModelManagementContext();
  const currentModelLabel = getModelDisplayName(settings?.current_model, models);
  const actionIdRef = useRef(0);
  const copiedTimerRef = useRef<number | null>(null);

  const clearCopyTimer = useCallback(() => {
    if (copiedTimerRef.current) {
      window.clearTimeout(copiedTimerRef.current);
      copiedTimerRef.current = null;
    }
  }, []);

  useEffect(() => {
    return () => {
      actionIdRef.current += 1;
      clearCopyTimer();
    };
  }, [clearCopyTimer]);

  const resetSubmitFallback = useCallback(() => {
    setSubmitError("");
    setFallbackReportData(null);
    setCopied(false);
    clearCopyTimer();
  }, [clearCopyTimer]);

  const handleSubmitReport = async () => {
    const trimmedMessage = message.trim();
    let isValid = true;

    if (!trimmedMessage) {
      setMessageError("Please describe the issue you are experiencing.");
      isValid = false;
    } else {
      setMessageError("");
    }

    if (!isValid) return;

    resetSubmitFallback();
    setSent(false);
    const actionId = actionIdRef.current + 1;
    actionIdRef.current = actionId;
    setIsSubmitting(true);

    try {
      let data: ManualReportData;
      try {
        data = await gatherManualReportData(
          undefined,
          undefined,
          trimmedMessage,
          currentModelLabel,
        );
      } catch (error) {
        if (actionId === actionIdRef.current) {
          log.error("Failed to gather report data:", error);
          toast.error("Failed to gather report data");
        }
        return;
      }

      if (actionId !== actionIdRef.current) return;

      const result = await submitManualReport(data);
      if (actionId !== actionIdRef.current) return;

      if (result.success) {
        setSent(true);
        setMessage("");
        toast.success("Report submitted. Thank you.");
        return;
      }

      const errorMessage =
        result.message || "Failed to submit report. You can copy the report and send it manually.";
      setSubmitError(errorMessage);
      setFallbackReportData(data);
      toast.error(errorMessage);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCopyReport = async () => {
    if (!fallbackReportData) return;
    const actionId = actionIdRef.current;

    try {
      await navigator.clipboard.writeText(buildReportBody(fallbackReportData));
      if (actionId !== actionIdRef.current) return;
      setCopied(true);
      toast.success("Report copied to clipboard");
      clearCopyTimer();
      copiedTimerRef.current = window.setTimeout(() => {
        setCopied(false);
        copiedTimerRef.current = null;
      }, 2000);
    } catch (error) {
      if (actionId !== actionIdRef.current) return;
      log.error("Failed to copy report:", error);
      toast.error("Failed to copy report");
    }
  };

  return (
    <SettingsPage className="max-w-none gap-[18px] px-7 pb-7 pl-6 pt-1">
      <PageHeader
        title="Help & feedback"
        description="Get unstuck fast, or tell us what broke."
        className="[&_h1]:leading-[normal] [&_h1]:tracking-[-0.4px] [&_p]:text-[13.5px] [&_p]:leading-[normal]"
      />
      <div className="grid grid-cols-1 items-start gap-3 sm:grid-cols-3">
        <ChoiceLink
          label="Troubleshooting"
          description="Permissions, quick fixes and reset."
          icon={Wrench}
          action="Open"
          onClick={() => onNavigateSettingsPane?.("advanced")}
        />
        <ChoiceLink
          label="What's new"
          description={
            version
              ? `See what changed in ${version}.`
              : "See what changed in your installed version."
          }
          icon={Sparkles}
          action="Read"
          disabled={!version}
          onClick={() => setAnnouncementOpen(true)}
        />
        <ChoiceLink
          label="Shortcuts"
          description="Every shortcut in one place."
          icon={Keyboard}
          action="Open"
          onClick={() => onNavigateSettingsPane?.("shortcuts")}
        />
      </div>
      <SettingsCard
        title="Report a problem"
        description="Tell us what happened. We never see your audio or transcripts."
        className="border-0 p-[18px] ring-1 ring-inset ring-border [&_h2]:text-sm [&_h2]:leading-[normal] [&>div:first-child_p]:mt-[3px] [&>div:first-child_p]:text-[12.5px] [&>div:first-child_p]:leading-[normal] [&>div+div]:mt-3"
      >
        <form
          className="flex flex-col gap-3"
          onSubmit={(event) => {
            event.preventDefault();
            void handleSubmitReport();
          }}
          noValidate
        >
          <label htmlFor="report-message" className="sr-only">
            What were you doing, and what went wrong?
          </label>
          <Textarea
            id="report-message"
            name="message"
            placeholder="What were you doing, and what went wrong?"
            value={message}
            onChange={(event) => {
              setMessage(event.target.value);
              if (messageError) setMessageError("");
              if (submitError) resetSubmitFallback();
            }}
            maxLength={5000}
            required
            disabled={isSubmitting}
            aria-invalid={Boolean(messageError)}
            aria-describedby={messageError ? "report-message-error" : "report-diagnostics-note"}
            className="h-24 min-h-24 resize-y rounded-[10px] border-0 bg-background p-3 text-[13px] leading-[normal] text-muted-foreground shadow-none ring-1 ring-inset ring-border placeholder:text-muted-foreground md:text-[13px]"
          />
          {messageError ? (
            <FieldError id="report-message-error" className="text-muted-foreground">
              {messageError}
            </FieldError>
          ) : null}
          <p
            id="report-diagnostics-note"
            className="min-h-5 text-[12.5px] leading-[normal] text-muted-foreground"
          >
            Automatically includes app version, current model, system details, device ID and recent
            redacted diagnostic logs; no audio or transcripts.
          </p>
          {submitError ? (
            <Alert variant="destructive">
              <AlertCircle />
              <AlertTitle>Report not sent</AlertTitle>
              <AlertDescription className="text-muted-foreground">
                {submitError} Copy the prepared report and send it manually if this keeps happening.
              </AlertDescription>
            </Alert>
          ) : null}
          <div className="flex items-center justify-between gap-3">
            <div
              role="status"
              className="flex items-center gap-2 text-[12.5px] leading-[normal] text-muted-foreground"
            >
              {sent ? (
                <>
                  <CircleCheck className="size-[15px] text-sage" />
                  Report submitted. Thank you.
                </>
              ) : null}
            </div>
            <div className="flex items-center gap-2">
              {fallbackReportData ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void handleCopyReport()}
                  disabled={isSubmitting}
                  className="text-muted-foreground"
                >
                  {copied ? <Check /> : <Copy />}
                  {copied ? "Copied" : "Copy report"}
                </Button>
              ) : null}
              <Button
                type="submit"
                disabled={isSubmitting}
                aria-busy={isSubmitting}
                className="h-auto rounded-[10px] bg-primary px-4 py-[9px] text-sm font-medium leading-[normal] text-background"
              >
                {isSubmitting ? "Submitting..." : "Send report"}
              </Button>
            </div>
          </div>
        </form>
      </SettingsCard>
      <UpdateAnnouncementDialog
        version={announcementOpen ? version : null}
        onClose={() => setAnnouncementOpen(false)}
      />
    </SettingsPage>
  );
}
