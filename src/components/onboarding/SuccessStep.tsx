import type { BareModifierSpec } from "@/components/HotkeyInput";
import { formatBareModifierLabel } from "@/components/onboarding/onboardingTypes";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { KeyCaps } from "@/components/KeyCaps";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { isMacOS } from "@/lib/platform";
import { useTestDictation } from "@/components/tabs/overview/useTestDictation";
import { CircleCheck } from "lucide-react";
import { useState, useEffect, useRef, type ReactNode } from "react";

export function SuccessStep({
  editor,
  onChangeShortcut,
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
  editor?: ReactNode;
  onChangeShortcut: () => void;
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
  const [text, setText] = useState("");
  const trialRef = useRef<HTMLTextAreaElement>(null);
  const editing = Boolean(editor);
  useEffect(() => {
    if (!editing) trialRef.current?.focus();
  }, [editing]);
  const { feedback, contentChanged } = useTestDictation(!editor);
  const caps = capturedBareModifier
    ? [formatBareModifierLabel(capturedBareModifier)]
    : shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows");
  return (
    <section className="flex w-full flex-col items-center gap-[22px]">
      <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">Try it now</h2>
      <p className="flex flex-wrap items-center justify-center gap-[10px] text-base text-muted-foreground">
        {holdToTalk ? "Hold" : "Press"} <KeyCaps caps={caps} size="onboarding" />
        {holdToTalk ? "and say anything, then let go." : "and say anything. Press again to stop."}
      </p>
      {editor}
      <div
        className={
          editor
            ? "hidden"
            : "flex h-[150px] w-full flex-col gap-[10px] rounded-[14px] outline-[1.5px] outline-sage outline-offset-[-0.75px] bg-card p-[18px]"
        }
      >
        <textarea
          ref={trialRef}
          autoFocus
          aria-label="Try a dictation"
          placeholder="Your words will appear here…"
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            contentChanged();
          }}
          className="min-h-0 flex-1 resize-none bg-transparent text-[17px] leading-[26px] text-foreground outline-none placeholder:text-muted-foreground"
        />
        {feedback ? (
          <p
            role="status"
            className="flex items-center gap-2 text-[12.5px] font-medium text-muted-foreground"
          >
            {feedback.startsWith("Worked") ? <CircleCheck className="size-4 text-sage" /> : null}
            {feedback}
          </p>
        ) : null}
      </div>
      <div className="flex items-center gap-[10px]">
        <Button
          variant="outline"
          className="h-auto rounded-[10px] px-4 py-[9px] text-[13px] leading-[normal] text-muted-foreground"
          disabled={Boolean(editor) || isSavingCompletion}
          onClick={onChangeShortcut}
        >
          Change shortcut
        </Button>
        <Button
          disabled={isSavingCompletion}
          className="h-auto rounded-[10px] px-4 py-[9px] border-0 text-sm leading-4"
          onClick={() => void onComplete()}
        >
          {isSavingCompletion ? <Spinner /> : null}Start using Voicetypr
        </Button>
      </div>
      <div className="flex w-full flex-col gap-3 text-left text-sm">
        <label className="flex items-start gap-3 rounded-[14px] border border-border bg-card p-4">
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

        <label className="flex items-start gap-3 rounded-[14px] border border-border bg-card p-4">
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
    </section>
  );
}
