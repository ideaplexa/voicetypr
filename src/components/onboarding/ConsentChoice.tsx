import { TELEMETRY_COPY, WhatsShared } from "@/components/WhatsShared";
import { openExternalLink } from "@/components/sections/accountLicenseUtils";

export function ConsentChoice({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <div className="flex w-full flex-col gap-2 text-left text-sm">
      <label className="flex items-start gap-3 rounded-[14px] border border-border bg-card p-4">
        <input
          type="checkbox"
          checked={checked}
          onChange={(event) => onChange(event.target.checked)}
          className="mt-0.5 size-4 shrink-0 accent-[var(--sage)]"
        />
        <span className="text-muted-foreground">{TELEMETRY_COPY}</span>
      </label>
      <p className="flex flex-wrap items-center justify-center gap-1 text-xs text-muted-foreground">
        <WhatsShared /> · By continuing you agree to the
        <button type="button" className="underline hover:text-foreground" onClick={() => void openExternalLink("https://voicetypr.com/terms")}>Terms</button>
        and
        <button type="button" className="underline hover:text-foreground" onClick={() => void openExternalLink("https://voicetypr.com/privacy")}>Privacy Policy</button>.
      </p>
    </div>
  );
}
