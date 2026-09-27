import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle, Bug, Check, Copy, Send } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { SettingsCard, SettingsHeader, SettingsPage } from "@/components/settings/settings-ui";
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
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export function ReportProblemSection() {
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [message, setMessage] = useState("");
  const [emailError, setEmailError] = useState("");
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
    const trimmedName = name.trim();
    const trimmedEmail = email.trim();
    const trimmedMessage = message.trim();
    let isValid = true;

    if (!trimmedEmail) {
      setEmailError("Enter an email address so we can follow up.");
      isValid = false;
    } else if (!EMAIL_PATTERN.test(trimmedEmail)) {
      setEmailError("Enter a valid email address.");
      isValid = false;
    } else {
      setEmailError("");
    }

    if (!trimmedMessage) {
      setMessageError("Please describe the issue you are experiencing.");
      isValid = false;
    } else {
      setMessageError("");
    }

    if (!isValid) return;

    resetSubmitFallback();
    const actionId = actionIdRef.current + 1;
    actionIdRef.current = actionId;
    setIsSubmitting(true);

    try {
      let data: ManualReportData;
      try {
        data = await gatherManualReportData(
          trimmedName || undefined,
          trimmedEmail,
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
        setName("");
        setEmail("");
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
    <SettingsPage>
      <SettingsHeader
        title="Report a problem"
        description="Tell us what happened and how to reach you. We'll attach the app version, your current model, system details, and recent diagnostic logs automatically."
      />

      <SettingsCard icon={Bug} title="Report details">
        <form
          className="mt-4"
          onSubmit={(event) => {
            event.preventDefault();
            void handleSubmitReport();
          }}
          noValidate
        >
          <FieldGroup className="gap-5">
            <FieldGroup className="grid gap-4 sm:grid-cols-2">
              <Field data-disabled={isSubmitting}>
                <FieldLabel htmlFor="report-name">Name (optional)</FieldLabel>
                <Input
                  id="report-name"
                  name="name"
                  autoComplete="name"
                  placeholder="Your name"
                  maxLength={200}
                  value={name}
                  disabled={isSubmitting}
                  onChange={(event) => {
                    setName(event.target.value);
                    if (submitError) resetSubmitFallback();
                  }}
                />
              </Field>

              <Field data-invalid={Boolean(emailError)} data-disabled={isSubmitting}>
                <FieldLabel htmlFor="report-email">Email</FieldLabel>
                <Input
                  id="report-email"
                  name="email"
                  type="email"
                  autoComplete="email"
                  placeholder="you@example.com"
                  maxLength={320}
                  required
                  value={email}
                  disabled={isSubmitting}
                  aria-invalid={Boolean(emailError)}
                  aria-describedby={emailError ? "report-email-error" : "report-email-description"}
                  onChange={(event) => {
                    setEmail(event.target.value);
                    if (emailError) setEmailError("");
                    if (submitError) resetSubmitFallback();
                  }}
                />
                {emailError ? (
                  <FieldError id="report-email-error">{emailError}</FieldError>
                ) : (
                  <FieldDescription id="report-email-description">
                    Used only to follow up about this report.
                  </FieldDescription>
                )}
              </Field>
            </FieldGroup>

            <Field data-invalid={Boolean(messageError)} data-disabled={isSubmitting}>
              <FieldLabel htmlFor="report-message">Describe the issue</FieldLabel>
              <Textarea
                id="report-message"
                name="message"
                placeholder="Tell us what happened..."
                value={message}
                onChange={(event) => {
                  setMessage(event.target.value);
                  if (messageError) setMessageError("");
                  if (submitError) resetSubmitFallback();
                }}
                rows={8}
                maxLength={5000}
                required
                disabled={isSubmitting}
                aria-invalid={Boolean(messageError)}
                aria-describedby={messageError ? "report-message-error" : "report-diagnostics-note"}
                className="min-h-40 resize-y"
              />
              {messageError ? (
                <FieldError id="report-message-error">{messageError}</FieldError>
              ) : (
                <FieldDescription id="report-diagnostics-note">
                  Include what you expected, what happened instead, and any steps that reproduce it.
                </FieldDescription>
              )}
            </Field>

            {submitError ? (
              <Alert variant="destructive">
                <AlertCircle />
                <AlertTitle>Report not sent</AlertTitle>
                <AlertDescription>
                  {submitError} Copy the prepared report and send it manually if this keeps
                  happening.
                </AlertDescription>
              </Alert>
            ) : null}

            <Field orientation="horizontal" className="justify-end">
              {fallbackReportData ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void handleCopyReport()}
                  disabled={isSubmitting}
                >
                  {copied ? <Check data-icon="inline-start" /> : <Copy data-icon="inline-start" />}
                  {copied ? "Copied" : "Copy report"}
                </Button>
              ) : null}
              <Button type="submit" size="sm" disabled={isSubmitting} aria-busy={isSubmitting}>
                <Send data-icon="inline-start" />
                {isSubmitting ? "Submitting..." : "Send report"}
              </Button>
            </Field>
          </FieldGroup>
        </form>
      </SettingsCard>
    </SettingsPage>
  );
}
