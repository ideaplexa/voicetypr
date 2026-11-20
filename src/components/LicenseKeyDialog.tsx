import { useId, useRef, useState, type FormEvent } from "react";
import { Button } from "@/components/settings/SettingsButton";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { openExternalLink } from "@/components/sections/accountLicenseUtils";

/** Where the user enters a license key: after Upgrade, and from "I have a key". */
export function LicenseKeyDialog({
  open,
  onOpenChange,
  onActivate,
  onRecheck,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onActivate: (key: string) => Promise<string | null>;
  onRecheck: () => Promise<void>;
}) {
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const [key, setKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"activate" | "recheck" | null>(null);

  const activate = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    const trimmed = key.trim();
    if (!trimmed) {
      setError("Paste your key first.");
      input.current?.focus();
      return;
    }
    setBusy("activate");
    setError(null);
    try {
      const failure = await onActivate(trimmed);
      if (failure) {
        setError(failure);
        input.current?.select();
      } else {
        setKey("");
        onOpenChange(false);
      }
    } finally {
      setBusy(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent initialFocus={input} className="sm:max-w-[440px]">
        <form noValidate onSubmit={(event) => void activate(event)} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>Activate your license</DialogTitle>
            <DialogDescription>Paste the key from your receipt email.</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={id}>License key</Label>
            <Input
              ref={input}
              id={id}
              autoComplete="off"
              spellCheck={false}
              value={key}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? `${id}-error` : undefined}
              onChange={(event) => {
                setKey(event.target.value);
                if (error) setError(null);
              }}
            />
            {error ? (
              <p id={`${id}-error`} role="alert" className="text-xs text-destructive">{error}</p>
            ) : null}
          </div>
          <p className="text-xs text-muted-foreground">
            Bought it but can’t find the key?{" "}
            <button
              type="button"
              className="underline hover:text-foreground"
              onClick={() => void openExternalLink("https://polar.sh/ideaplexa/portal")}
            >
              Manage license
            </button>{" "}
            lists every key you bought.
          </p>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              disabled={busy !== null}
              onClick={() => {
                setBusy("recheck");
                void onRecheck().finally(() => setBusy(null));
              }}
            >
              {busy === "recheck" ? <Spinner /> : null}Recheck
            </Button>
            <Button type="submit" disabled={busy !== null} aria-busy={busy === "activate"}>
              {busy === "activate" ? <Spinner /> : null}Activate
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
