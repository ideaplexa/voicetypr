import { TELEMETRY_COPY, WhatsShared } from "@/components/WhatsShared";
import type { BareModifierSpec } from "@/components/HotkeyInput";
import { formatBareModifierLabel } from "@/components/onboarding/onboardingTypes";
import { Button } from "@/components/settings/SettingsButton";
import { Spinner } from "@/components/ui/spinner";
import { KeyCaps } from "@/components/KeyCaps";
import { shortcutKeyCaps } from "@/lib/shortcut-key-caps";
import { isMacOS } from "@/lib/platform";
import { useTestDictation } from "@/components/tabs/overview/useTestDictation";
import { CircleCheck } from "lucide-react";
import { useState, useEffect, useRef, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { RecordingStatePayload } from "@/types";
import { toast } from "sonner";

export function SuccessStep({
  editor,
  onChangeShortcut,
  capturedBareModifier,
  holdToTalk,
  hotkey,
  telemetryOptIn,
  isSavingCompletion,
  onTelemetryChange,
  onComplete,
  onStartTrial,
}: {
  editor?: ReactNode;
  onChangeShortcut: () => void;
  capturedBareModifier: BareModifierSpec | null;
  holdToTalk: boolean;
  hotkey: string;
  telemetryOptIn: boolean;
  isSavingCompletion: boolean;
  onTelemetryChange: (checked: boolean) => void;
  onComplete: () => void | Promise<void>;
  onStartTrial?: () => Promise<void>;
}) {
  const [text, setText] = useState("");
  const trialRef = useRef<HTMLTextAreaElement>(null);
  const [recording, setRecording] = useState(false);
  const [stopping, setStopping] = useState(false);
  useEffect(() => {
    let disposed = false;
    let revision = 0;
    const unlisten = listen<RecordingStatePayload>("recording-state-changed", ({ payload }) => {
      revision++;
      if (!disposed) setRecording(payload.state === "recording" || payload.state === "starting");
    });
    void unlisten.then(async () => {
      const observed = revision;
      const current = await invoke<RecordingStatePayload>("get_current_recording_state");
      if (!disposed && revision === observed) setRecording(current.state === "recording" || current.state === "starting");
    }).catch(() => undefined);
    return () => { disposed = true; void unlisten.then((stop) => stop()); };
  }, []);
  const editing = Boolean(editor);
  // Dictation needs a trial or a license, so the practice box waits for the click.
  const trialGate = Boolean(onStartTrial) && !editing;
  const [startingTrial, setStartingTrial] = useState(false);
  useEffect(() => {
    if (!editing && !trialGate) trialRef.current?.focus();
  }, [editing, trialGate]);
  const { feedback, contentChanged } = useTestDictation(!editing && !trialGate);
  const caps = capturedBareModifier
    ? [formatBareModifierLabel(capturedBareModifier)]
    : shortcutKeyCaps(hotkey, isMacOS ? "darwin" : "windows");
  return (
    <section className="flex w-full flex-col items-center gap-[22px]">
      <h2 className="text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        {trialGate ? "Start your free trial" : "Try it now"}
      </h2>
      {trialGate ? (
        <>
          <p className="text-center text-base text-muted-foreground">
            Your trial begins when you start it. Have a license key? Finish setup, then enter it under License.
          </p>
          <Button
            disabled={startingTrial}
            onClick={() => {
              setStartingTrial(true);
              void onStartTrial?.().finally(() => setStartingTrial(false));
            }}
          >
            {startingTrial ? <Spinner /> : null}Start free trial
          </Button>
        </>
      ) : (
        <p className="flex flex-wrap items-center justify-center gap-[10px] text-base text-muted-foreground">
          {holdToTalk ? "Hold" : "Press"} <KeyCaps caps={caps} size="onboarding" />
          {holdToTalk ? "and say anything, then let go." : "and say anything. Press again to stop."}
        </p>
      )}
      {recording && !editor && (
        <Button disabled={stopping} onClick={() => {
          setStopping(true);
          void invoke("stop_recording").catch(() => toast.error("Could not stop dictation. Try again."))
            .finally(() => setStopping(false));
        }}>Stop</Button>
      )}
      {editor}
      <div
        className={
          editor || trialGate
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
      <div className="flex w-full flex-col gap-3 text-left text-sm">
        <label className="flex items-start gap-3 rounded-[14px] border border-border bg-card p-4">
          <input type="checkbox" checked={telemetryOptIn} onChange={(event) => onTelemetryChange(event.target.checked)}
            className="mt-0.5 size-4 shrink-0 accent-[var(--sage)]" />
          <span className="text-muted-foreground">{TELEMETRY_COPY}</span>
        </label>
        <WhatsShared />
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
    </section>
  );
}
