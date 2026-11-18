import { useId, useRef, useState, type FormEvent, type ReactNode } from "react";
import { Button } from "@/components/settings/SettingsButton";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import type { TrialStartError } from "@/types";
import { cn } from "@/lib/utils";
import { TRIAL_NOTICE } from "@/lib/trial-notice";
import { openExternalLink } from "@/components/sections/accountLicenseUtils";

const NAME_MAX_LENGTH = 100;
const EMAIL_MAX_LENGTH = 254;
const EMAIL_PATTERN = /^(?!\.)(?!.*\.\.)([A-Za-z0-9_'+\-.]*)[A-Za-z0-9_+-]@([A-Za-z0-9][A-Za-z0-9-]*\.)+[A-Za-z]{2,}$/;
const CONTROL_CHARACTERS = /\p{Cc}/u;

type FieldErrors = { name?: string; email?: string };

const SERVER_MESSAGES: Record<Exclude<TrialStartError, "trial_invalid_name" | "trial_invalid_email" | "trial_email_used">, string> = {
  trial_unreachable: "Couldn’t reach Voicetypr, so your trial hasn’t started. Check your connection and try again.",
  trial_rate_limited: "Too many attempts, so your trial hasn’t started. Wait a few minutes, then try again.",
  trial_failed: "We couldn’t confirm your trial. Try again in a moment; pressing again never starts a second trial.",
};

export function validateTrialContact(name: string, email: string): FieldErrors {
  const errors: FieldErrors = {};
  if (!name) errors.name = "Enter your name";
  else if (name.length > NAME_MAX_LENGTH) errors.name = `Use at most ${NAME_MAX_LENGTH} characters`;
  else if (CONTROL_CHARACTERS.test(name)) errors.name = "Use letters, spaces and punctuation only";
  if (!email) errors.email = "Enter your email";
  else if (email.length > EMAIL_MAX_LENGTH || !EMAIL_PATTERN.test(email)) errors.email = "That email doesn’t look right";
  return errors;
}

export function TrialStartForm({
  onStart,
  className,
  large,
  renderAction,
}: {
  onStart: (name: string, email: string) => Promise<TrialStartError | null>;
  className?: string;
  /** Full-size fields and a full-width button, for onboarding. */
  large?: boolean;
  /** Places the start button and the email notice, e.g. inside a choice card. */
  renderAction?: (button: ReactNode, notice: ReactNode) => ReactNode;
}) {
  const id = useId();
  const nameRef = useRef<HTMLInputElement>(null);
  const emailRef = useRef<HTMLInputElement>(null);
  const pending = useRef(false);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [errors, setErrors] = useState<FieldErrors>({});
  const [serverError, setServerError] = useState<string | null>(null);
  const [emailUsed, setEmailUsed] = useState(false);
  const [starting, setStarting] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (pending.current) return;
    const trimmedName = name.trim();
    const trimmedEmail = email.trim();
    const found = validateTrialContact(trimmedName, trimmedEmail);
    setErrors(found);
    setServerError(null);
    setEmailUsed(false);
    if (found.name) return nameRef.current?.focus();
    if (found.email) return emailRef.current?.focus();
    pending.current = true;
    setStarting(true);
    try {
      const error = await onStart(trimmedName, trimmedEmail);
      if (error === "trial_invalid_name") {
        setErrors({ name: "That name doesn’t look right" });
        nameRef.current?.focus();
      } else if (error === "trial_invalid_email") {
        setErrors({ email: "That email doesn’t look right" });
        emailRef.current?.focus();
      } else if (error === "trial_email_used") {
        setEmailUsed(true);
      } else if (error) {
        setServerError(SERVER_MESSAGES[error]);
      }
    } finally {
      pending.current = false;
      setStarting(false);
    }
  };

  const field = (
    key: keyof FieldErrors,
    label: string,
    props: { ref: typeof nameRef; value: string; onChange: (value: string) => void; type: string; autoComplete: string; placeholder: string },
  ) => (
    <div className="flex min-w-0 flex-1 flex-col gap-1.5 text-left">
      <Label htmlFor={`${id}-${key}`} className={large ? "text-[13px]" : "text-[12.5px] text-muted-foreground"}>{label}</Label>
      <Input
        ref={props.ref}
        id={`${id}-${key}`}
        type={props.type}
        autoComplete={props.autoComplete}
        placeholder={props.placeholder}
        className={large ? "h-11 px-4 md:text-[15px]" : undefined}
        value={props.value}
        aria-invalid={errors[key] ? true : undefined}
        aria-describedby={errors[key] ? `${id}-${key}-error` : undefined}
        onChange={(event) => {
          props.onChange(event.target.value);
          if (errors[key]) setErrors((current) => ({ ...current, [key]: undefined }));
        }}
      />
      {errors[key] ? (
        <p id={`${id}-${key}-error`} className="text-xs text-destructive">{errors[key]}</p>
      ) : null}
    </div>
  );

  return (
    <form noValidate onSubmit={(event) => void submit(event)} className={cn("flex w-full flex-col items-center gap-3", className)}>
      <div className="flex w-full flex-col gap-2.5 sm:flex-row">
        {field("name", "Name", { ref: nameRef, value: name, onChange: setName, type: "text", autoComplete: "name", placeholder: "Priya Raman" })}
        {field("email", "Email", { ref: emailRef, value: email, onChange: setEmail, type: "email", autoComplete: "email", placeholder: "priya@example.com" })}
      </div>
      {(renderAction ?? ((button: ReactNode, notice: ReactNode) => <>{button}{notice}</>))(
        <Button type="submit" size={large ? "lg" : undefined} className={large ? "w-full" : undefined} disabled={starting} aria-busy={starting}>
          {starting ? <Spinner /> : null}Start 7-day trial
        </Button>,
        <p className="max-w-[460px] text-center text-[12.5px] text-muted-foreground">{TRIAL_NOTICE}</p>,
      )}
      {emailUsed ? (
        <div role="alert" className="flex max-w-[460px] flex-col items-center gap-2 text-center text-[13px]">
          <p className="font-medium text-foreground">This email has already been used for a trial</p>
          <p className="text-muted-foreground">
            To keep using Voicetypr, buy a license. If you think this is a mistake, write to support@voicetypr.com.
          </p>
          <div className="flex gap-2">
            <Button type="button" size="sm" onClick={() => void openExternalLink("https://voicetypr.com/#pricing")}>
              Buy Voicetypr
            </Button>
            <Button type="button" size="sm" variant="secondary" onClick={() => void openExternalLink("mailto:support@voicetypr.com")}>
              Write to us
            </Button>
          </div>
        </div>
      ) : null}
      {serverError ? (
        <p role="alert" className="max-w-[460px] text-center text-[13px] text-destructive">{serverError}</p>
      ) : null}
    </form>
  );
}
