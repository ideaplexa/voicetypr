import { KeyCaps } from "@/components/KeyCaps";
import { MicrophoneCheck } from "@/components/onboarding/MicrophoneCheck";
import type { PermissionState } from "@/components/onboarding/onboardingTypes";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";
import { ChoiceCard, SettingsPaneCard } from "@/components/settings/settings-ui";
import { Button } from "@/components/settings/SettingsButton";
import { isMacOS } from "@/lib/platform";
import { Keyboard } from "lucide-react";
import type { ReactNode } from "react";

export function SetupStep({
  caps,
  holdToTalk,
  onHoldToTalkChange,
  permissions,
  checkingPermissions,
  isRequestingPermission,
  onCheck,
  onRequest,
  editor,
  onChangeShortcut,
  onBack,
  onSkip,
  onSkipAccessibility,
  onNext,
  nextDisabled,
  saving,
}: {
  caps: string[];
  holdToTalk: boolean;
  onHoldToTalkChange: (hold: boolean) => void;
  permissions: { microphone: PermissionState; accessibility: PermissionState };
  checkingPermissions: Set<string>;
  isRequestingPermission: string | null;
  onCheck: (type: "microphone" | "accessibility") => void | Promise<void>;
  onRequest: (type: "microphone" | "accessibility") => void | Promise<void>;
  /** The shortcut editor while the user is changing it. */
  editor: ReactNode;
  onChangeShortcut: () => void;
  onBack: () => void;
  /** Moves on without saving a shortcut change. */
  onSkip: () => void;
  onSkipAccessibility: () => void;
  onNext: () => void;
  nextDisabled: boolean;
  saving: boolean;
}) {
  const accessibilityAllowed = permissions.accessibility.status === "granted";
  return (
    <section className="flex w-full flex-col items-center gap-[18px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        Now let it type anywhere
      </h2>
      <p className="text-center text-sm text-muted-foreground">
        {isMacOS
          ? "Your microphone, one permission and your shortcut. Then Voicetypr works in every app."
          : "Your microphone and your shortcut. Then Voicetypr works in every app."}
      </p>
      {isMacOS ? (
        <PermissionsStep
          permissions={permissions}
          checkingPermissions={checkingPermissions}
          isRequestingPermission={isRequestingPermission}
          onCheck={onCheck}
          onRequest={onRequest}
        />
      ) : null}
      {!isMacOS || permissions.microphone.status === "granted" ? <MicrophoneCheck /> : null}
      {editor ?? (
        <SettingsPaneCard className="w-full !px-[18px] !py-[14px]">
          <div className="flex items-center gap-[14px]">
            <span className="flex size-[34px] shrink-0 items-center justify-center rounded-[9px] bg-sage-bg">
              <Keyboard className="size-[18px] text-sage" />
            </span>
            <div className="flex-1 space-y-0.5">
              <h3 className="text-sm font-semibold">Shortcut</h3>
              <p className="text-[12.5px] text-muted-foreground">
                {holdToTalk ? "Hold to talk, let go to " : "Press to talk, press again to "}
                {isMacOS && !accessibilityAllowed ? "copy" : "paste"}.
              </p>
            </div>
            <KeyCaps caps={caps} size="onboarding" />
            <Button variant="ghost" size="sm" onClick={onChangeShortcut}>Change</Button>
          </div>
        </SettingsPaneCard>
      )}
      {!editor ? (
        <div role="group" aria-label="How the shortcut works" className="grid w-full gap-2.5 sm:grid-cols-2">
          <ChoiceCard
            layout="row"
            label="Press to start and stop"
            description="Press once to talk, press again to finish."
            selected={!holdToTalk}
            onSelect={() => onHoldToTalkChange(false)}
          />
          <ChoiceCard
            layout="row"
            label="Hold to talk"
            description="Talk while you hold it, let go to finish."
            selected={holdToTalk}
            onSelect={() => onHoldToTalkChange(true)}
          />
        </div>
      ) : null}
      {!editor ? (
        <div className="flex flex-wrap justify-center gap-[10px]">
          <Button variant="outline" onClick={onBack}>Back</Button>
          {isMacOS && !accessibilityAllowed ? (
            <Button variant="ghost" disabled={saving} onClick={onSkipAccessibility}>Skip, I’ll paste myself</Button>
          ) : (
            <Button variant="ghost" disabled={saving} onClick={onSkip}>Skip for now</Button>
          )}
          <Button disabled={nextDisabled || saving} onClick={onNext}>Continue</Button>
        </div>
      ) : null}
    </section>
  );
}
