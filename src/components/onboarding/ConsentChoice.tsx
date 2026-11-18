import { TELEMETRY_COPY, WhatsShared } from "@/components/WhatsShared";
import { openExternalLink } from "@/components/sections/accountLicenseUtils";

/** "By … you agree to the Terms and Privacy Policy." */
export function TermsLine({ action }: { action: string }) {
  return (
    <>
      By {action} you agree to the
      <button type="button" className="underline hover:text-foreground" onClick={() => void openExternalLink("https://voicetypr.com/terms")}>Terms</button>
      and
      <button type="button" className="underline hover:text-foreground" onClick={() => void openExternalLink("https://voicetypr.com/privacy")}>Privacy Policy</button>.
    </>
  );
}

export function ConsentChoice({ checked, onChange, disabled, terms }: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  /** The agreement line, when the trial step has not already shown it. */
  terms: boolean;
}) {
  return (
    <div className="flex w-full flex-col gap-2 text-left text-sm">
      <label className="flex items-start gap-3 rounded-[14px] border border-border bg-card p-4">
        <input
          type="checkbox"
          checked={checked}
          disabled={disabled}
          onChange={(event) => onChange(event.target.checked)}
          className="mt-0.5 size-4 shrink-0 accent-[var(--sage)]"
        />
        <span className="text-muted-foreground">{TELEMETRY_COPY}</span>
      </label>
      <p className="flex flex-wrap items-center justify-center gap-1 text-xs text-muted-foreground">
        <WhatsShared />{terms ? <> · <TermsLine action="continuing" /></> : null}
      </p>
    </div>
  );
}
