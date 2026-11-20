import { validateTrialContact } from "@/components/TrialStartForm";
import { FrameHeading, PrimaryButton, SecondaryButton } from "@/components/onboarding/Frame";
import { TrialUnreachableStep } from "@/components/onboarding/LicenceStates";
import { openExternalLink } from "@/components/sections/accountLicenseUtils";
import { TRIAL_NOTICE } from "@/lib/trial-notice";
import type { TrialStartError } from "@/types";
import { cn } from "@/lib/utils";
import { useId, useRef, useState, type FormEvent } from "react";

type Errors = { name?: string; email?: string };

/** 12 Start your 7-day trial: name and email, only after "Start 7-day trial". E1 when Voicetypr can't be reached. */
export function TrialStep({
  pending,
  onStartTrial,
  onHaveKey,
  unreachable,
  onUnreachableChange,
}: {
  pending: boolean;
  onStartTrial: (name: string, email: string) => Promise<TrialStartError | null>;
  onHaveKey: () => void;
  /** E1: the last start couldn't reach Voicetypr. Back returns to the form. */
  unreachable: boolean;
  onUnreachableChange: (unreachable: boolean) => void;
}) {
  const id = useId();
  const nameRef = useRef<HTMLInputElement>(null);
  const emailRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [errors, setErrors] = useState<Errors>({});
  const [problem, setProblem] = useState<"rate_limited" | "email_used" | null>(null);

  const start = async () => {
    const trimmedName = name.trim();
    const trimmedEmail = email.trim();
    const found = validateTrialContact(trimmedName, trimmedEmail);
    setErrors(found);
    if (found.name) return nameRef.current?.focus();
    if (found.email) return emailRef.current?.focus();
    setProblem(null);
    const error = await onStartTrial(trimmedName, trimmedEmail);
    onUnreachableChange(error === "trial_unreachable" || error === "trial_failed");
    if (error === "trial_invalid_name") setErrors({ name: "That name doesn’t look right" });
    else if (error === "trial_invalid_email") setErrors({ email: "That email doesn’t look right" });
    else if (error === "trial_email_used") setProblem("email_used");
    else if (error === "trial_rate_limited") setProblem("rate_limited");
  };

  if (unreachable) {
    return <TrialUnreachableStep pending={pending} onTryAgain={() => void start()} onHaveKey={onHaveKey} />;
  }

  const field = (key: keyof Errors, label: string, ref: typeof nameRef, value: string, set: (value: string) => void, type: string, autoComplete: string) => (
    <div className="flex flex-col gap-[7px] self-stretch">
      <label htmlFor={`${id}-${key}`} className="text-sm font-medium text-foreground">{label}</label>
      <input
        ref={ref}
        id={`${id}-${key}`}
        type={type}
        autoComplete={autoComplete}
        value={value}
        aria-invalid={errors[key] ? true : undefined}
        aria-describedby={errors[key] ? `${id}-${key}-error` : undefined}
        onChange={(event) => {
          set(event.target.value);
          if (errors[key]) setErrors((current) => ({ ...current, [key]: undefined }));
        }}
        className={cn(
          "h-[46px] rounded-[10px] border bg-card px-3.5 text-[15px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-sage/30",
          errors[key] ? "border-[1.5px] border-destructive" : "border-border",
        )}
      />
      {errors[key] ? <p id={`${id}-${key}-error`} className="text-[13px] text-destructive">{errors[key]}</p> : null}
    </div>
  );

  return (
    <form
      noValidate
      onSubmit={(event: FormEvent) => {
        event.preventDefault();
        if (!pending) void start();
      }}
      className="flex w-[420px] flex-col items-center gap-[26px]"
    >
      <FrameHeading title="Start your 7-day trial" line="No card. It starts when you click." />
      <div className="flex flex-col gap-4 self-stretch">
        {field("name", "Name", nameRef, name, setName, "text", "name")}
        {field("email", "Email", emailRef, email, setEmail, "email", "email")}
      </div>
      <p className="self-stretch text-center text-sm leading-[1.45] text-muted-foreground">We ask so Moinul can reach you. {TRIAL_NOTICE}</p>
      <PrimaryButton type="submit" className="self-stretch" disabled={pending} aria-busy={pending}>Start trial</PrimaryButton>
      {problem === "rate_limited" ? (
        <p role="alert" className="text-center text-[13px] text-destructive">Too many attempts, so your trial hasn’t started. Wait a few minutes, then try again.</p>
      ) : null}
      {problem === "email_used" ? (
        <div role="alert" className="flex flex-col items-center gap-2 text-center text-[13px]">
          <p className="font-medium text-foreground">This email has already been used for a trial</p>
          <p className="text-muted-foreground">To keep using Voicetypr, buy a license. If you think this is a mistake, write to support@voicetypr.com.</p>
          <SecondaryButton onClick={() => void openExternalLink("https://voicetypr.com/#pricing")}>Buy Voicetypr</SecondaryButton>
        </div>
      ) : null}
    </form>
  );
}

/** Under 12: the founder line and the agreement. */
export function TrialFooter() {
  const link = (href: string, text: string) => (
    <button type="button" className="text-muted-foreground underline-offset-2 hover:underline" onClick={() => void openExternalLink(href)}>
      {text}
    </button>
  );
  return (
    <div className="flex flex-col items-center gap-1.5">
      <p className="text-[13.5px] text-muted-foreground">Made by Moinul. I read every email.</p>
      <p className="flex gap-1 text-[12.5px] text-text-3">
        By starting you agree to the {link("https://voicetypr.com/terms", "Terms")} and {link("https://voicetypr.com/privacy", "Privacy Policy.")}
      </p>
    </div>
  );
}
