import { IconTile } from "@/components/onboarding/LicenceStates";
import { FrameHeading, PrimaryButton, QuietButton } from "@/components/onboarding/Frame";
import { cn } from "@/lib/utils";
import { ArrowUpRight, KeyRound } from "lucide-react";
import { useState, type FormEvent } from "react";

/** E3 Enter your licence key, E4 when it didn't work. A key that activates goes to Done. */
export function KeyStep({ onActivate, onBuy }: { onActivate: (key: string) => Promise<string | null>; onBuy: () => void }) {
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    const trimmed = key.trim();
    if (!trimmed || pending) return;
    setPending(true);
    setError(null);
    try {
      setError(await onActivate(trimmed));
    } finally {
      setPending(false);
    }
  };
  return (
    <form noValidate onSubmit={(event) => void submit(event)} className="flex w-[440px] flex-col items-center gap-[30px]">
      <IconTile icon={KeyRound} tone="sage" />
      <FrameHeading title="Enter your licence key" line="Paste the key from your receipt email." />
      <div className="flex flex-col items-start gap-2 self-stretch">
        <input
          value={key}
          onChange={(event) => {
            setKey(event.target.value);
            setError(null);
          }}
          aria-label="Licence key"
          autoFocus
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? "licence-key-error" : undefined}
          autoComplete="off"
          spellCheck={false}
          placeholder="VT-XXXX-XXXX-XXXX"
          className={cn(
            "h-[46px] self-stretch rounded-[10px] border bg-card px-3.5 text-[15px] text-foreground outline-none focus-visible:ring-2 focus-visible:ring-sage/30",
            error ? "border-[1.5px] border-destructive" : "border-border",
          )}
        />
        {error ? <p id="licence-key-error" role="alert" className="text-sm text-destructive">{error}</p> : null}
      </div>
      <PrimaryButton type="submit" className="self-stretch" disabled={pending || !key.trim()}>Activate</PrimaryButton>
      <QuietButton className="text-sage hover:text-sage" onClick={onBuy}>
        <ArrowUpRight aria-hidden="true" className="size-[15px]" />
        Buy on voicetypr.com
      </QuietButton>
    </form>
  );
}
