import { useId, useRef, useState, type FormEvent } from "react";
import { Button } from "@/components/settings/SettingsButton";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import type { TrialStartError } from "@/types";
import { cn } from "@/lib/utils";

const NAME_MAX_LENGTH = 100;
const EMAIL_MAX_LENGTH = 254;
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const CONTROL_CHARACTERS = /\p{Cc}/u;

type FieldErrors = { name?: string; email?: string };

const SERVER_MESSAGES: Record<Exclude<TrialStartError, "trial_invalid_name" | "trial_invalid_email">, string> = {
  trial_unreachable: "Couldn’t reach Voicetypr, so your trial hasn’t started. Check your connection and try again.",
  trial_rate_limited: "Too many attempts, so your trial hasn’t started. Wait a few minutes, then try again.",
  trial_failed: "Something went wrong on our side, so your trial hasn’t started. Try again in a moment.",
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
}: {
  onStart: (name: string, email: string) => Promise<TrialStartError | null>;
  className?: string;
}) {
  const id = useId();
  const nameRef = useRef<HTMLInputElement>(null);
  const emailRef = useRef<HTMLInputElement>(null);
  const pending = useRef(false);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [errors, setErrors] = useState<FieldErrors>({});
  const [serverError, setServerError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (pending.current) return;
    const trimmedName = name.trim();
    const trimmedEmail = email.trim();
    const found = validateTrialContact(trimmedName, trimmedEmail);
    setErrors(found);
    setServerError(null);
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
      <Label htmlFor={`${id}-${key}`} className="text-[12.5px] text-muted-foreground">{label}</Label>
      <Input
        ref={props.ref}
        id={`${id}-${key}`}
        type={props.type}
        autoComplete={props.autoComplete}
        placeholder={props.placeholder}
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
      <Button type="submit" disabled={starting} aria-busy={starting}>
        {starting ? <Spinner /> : null}Start 7-day trial
      </Button>
      {serverError ? (
        <p role="alert" className="max-w-[460px] text-center text-[13px] text-destructive">{serverError}</p>
      ) : null}
    </form>
  );
}
