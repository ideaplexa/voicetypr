import type { BareModifierSpec } from "@/components/HotkeyInput";
import { formatBareModifierLabel } from "@/components/onboarding/onboardingTypes";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { formatHotkey } from "@/lib/hotkey-utils";
import { ShieldCheck } from "lucide-react";

export function SuccessStep({
  capturedBareModifier,
  holdToTalk,
  hotkey,
  telemetryOptIn,
  analyticsOptIn,
  isSavingCompletion,
  onTelemetryChange,
  onAnalyticsChange,
  onComplete,
}: {
  capturedBareModifier: BareModifierSpec | null;
  holdToTalk: boolean;
  hotkey: string;
  telemetryOptIn: boolean;
  analyticsOptIn: boolean;
  isSavingCompletion: boolean;
  onTelemetryChange: (checked: boolean) => void;
  onAnalyticsChange: (checked: boolean) => void;
  onComplete: () => void | Promise<void>;
}) {
  return (
    <section className="mx-auto flex w-full max-w-xl flex-col items-center gap-6 text-center">
      <div className="flex size-16 items-center justify-center rounded-3xl bg-sage text-sage-foreground shadow-sm">
        <ShieldCheck className="size-8" />
      </div>
      <div className="flex flex-col gap-3">
        <h1 className="text-4xl font-semibold tracking-[-0.04em]">You're all set</h1>
        <p className="text-muted-foreground">
          Voicetypr is ready to use.{" "}
          {capturedBareModifier ? (
            holdToTalk ? (
              <>
                Hold {formatBareModifierLabel(capturedBareModifier)} anywhere to start recording;
                release to stop.
              </>
            ) : (
              <>
                Tap {formatBareModifierLabel(capturedBareModifier)} anywhere to start or stop
                recording.
              </>
            )
          ) : holdToTalk ? (
            <>Hold {formatHotkey(hotkey)} anywhere to start recording; release to stop.</>
          ) : (
            <>Press {formatHotkey(hotkey)} anywhere to start recording.</>
          )}
        </p>
      </div>

      <p className="w-full rounded-2xl border border-border bg-card p-4 text-left text-sm text-muted-foreground shadow-sm">
        Tip: turn on Polish in Settings to clean up your dictation automatically.
      </p>

      <div className="flex w-full flex-col gap-3 text-left text-sm">
        <label className="flex items-start gap-3 rounded-2xl border border-border bg-card p-4 shadow-sm">
          <input
            type="checkbox"
            checked={telemetryOptIn}
            onChange={(event) => onTelemetryChange(event.target.checked)}
            className="mt-0.5 size-4 shrink-0 accent-[var(--sage)]"
          />
          <span className="text-muted-foreground">
            <strong className="block font-medium text-foreground">
              Crash &amp; error reporting
            </strong>
            Anonymous crash details go to GlitchTip. No audio, transcripts, clipboard contents, or
            prompts.
          </span>
        </label>

        <label className="flex items-start gap-3 rounded-2xl border border-border bg-card p-4 shadow-sm">
          <input
            type="checkbox"
            checked={analyticsOptIn}
            onChange={(event) => onAnalyticsChange(event.target.checked)}
            className="mt-0.5 size-4 shrink-0 accent-[var(--sage)]"
          />
          <span className="text-muted-foreground">
            <strong className="block font-medium text-foreground">Usage analytics</strong>
            Anonymous feature usage, outcomes, and performance buckets with PostHog. No session
            replay.
          </span>
        </label>
      </div>

      <Button size="lg" disabled={isSavingCompletion} onClick={() => void onComplete()}>
        {isSavingCompletion ? <Spinner /> : null}
        Start using Voicetypr
      </Button>
    </section>
  );
}
