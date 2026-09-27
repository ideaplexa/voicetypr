import { HotkeyInput, type BareModifierSpec } from "@/components/HotkeyInput";
import { OnboardingPanel, StepFooter } from "@/components/onboarding/OnboardingChrome";
import {
  formatBareModifierLabel,
  ONBOARDING_HOTKEY_VALIDATION,
} from "@/components/onboarding/onboardingTypes";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { Info, Keyboard } from "lucide-react";

export function HotkeyStep({
  hotkey,
  holdToTalk,
  capturedBareModifier,
  onHotkeyChange,
  onEditingChange,
  onBareModifier,
  onHoldToTalkChange,
  onBack,
  onNext,
  nextDisabled,
}: {
  hotkey: string;
  holdToTalk: boolean;
  capturedBareModifier: BareModifierSpec | null;
  onHotkeyChange: (value: string) => void;
  onEditingChange: (editing: boolean) => void;
  onBareModifier: (spec: BareModifierSpec) => void;
  onHoldToTalkChange: (checked: boolean) => void;
  onBack: () => void;
  onNext: () => void | Promise<void>;
  nextDisabled: boolean;
}) {
  return (
    <OnboardingPanel
      title="Pick your hotkey and recording mode"
      description="This is the system-wide shortcut for triggering Voicetypr. You can change both later in Settings."
      footer={
        <StepFooter
          onBack={onBack}
          onNext={onNext}
          nextDisabled={nextDisabled}
          nextLabel="Save hotkey"
        />
      }
    >
      <Card className="mx-auto w-full max-w-xl rounded-2xl border border-border bg-card shadow-sm">
        <CardHeader>
          <CardTitle className="flex items-center gap-2.5">
            <span className="flex size-8 items-center justify-center rounded-lg bg-sage-bg text-sage">
              <Keyboard className="size-4" />
            </span>
            Recording hotkey
          </CardTitle>
          <CardDescription>Double tap Esc cancels an active recording.</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <HotkeyInput
            value={hotkey}
            onChange={onHotkeyChange}
            onEditingChange={onEditingChange}
            onBareModifier={onBareModifier}
            allowBareModifier
            validationRules={ONBOARDING_HOTKEY_VALIDATION}
            placeholder={
              capturedBareModifier
                ? holdToTalk
                  ? `Hold ${formatBareModifierLabel(capturedBareModifier)} · push-to-talk`
                  : `Tap ${formatBareModifierLabel(capturedBareModifier)} · toggle on/off`
                : undefined
            }
          />
          {capturedBareModifier ? (
            holdToTalk ? (
              <Alert>
                <Info className="size-4" />
                <AlertTitle>Hold to talk</AlertTitle>
                <AlertDescription>
                  Hold {formatBareModifierLabel(capturedBareModifier)} anywhere to start recording —
                  release to stop.
                </AlertDescription>
              </Alert>
            ) : (
              <Alert>
                <Info className="size-4" />
                <AlertTitle>Tap to toggle on/off</AlertTitle>
                <AlertDescription>
                  Tap {formatBareModifierLabel(capturedBareModifier)} to start recording, tap again
                  to stop.
                </AlertDescription>
              </Alert>
            )
          ) : null}
          <div className="flex items-center gap-3">
            <Switch id="hold-to-talk" checked={holdToTalk} onCheckedChange={onHoldToTalkChange} />
            <label htmlFor="hold-to-talk" className="text-sm cursor-pointer select-none">
              Hold to talk (push-to-talk)
            </label>
          </div>
        </CardContent>
      </Card>
    </OnboardingPanel>
  );
}
