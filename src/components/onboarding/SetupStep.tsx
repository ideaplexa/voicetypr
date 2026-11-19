import { KeyCaps } from "@/components/KeyCaps";
import { MicrophoneCheck } from "@/components/onboarding/MicrophoneCheck";
import type { PermissionState } from "@/components/onboarding/onboardingTypes";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";
import { ChoiceCard, SettingsPaneCard } from "@/components/settings/settings-ui";
import { Button } from "@/components/settings/SettingsButton";
import { Progress } from "@/components/ui/progress";
import { isMacOS } from "@/lib/platform";
import { Keyboard } from "lucide-react";
import type { ReactNode } from "react";

export function SetupStep({
  part,
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
  /** Permissions (microphone and Accessibility) or the shortcut, each its own page. */
  part: "permissions" | "shortcut";
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
  const checks = [permissions.microphone.status === "granted", accessibilityAllowed];
  const done = checks.filter(Boolean).length;
  const permissionsPage = part === "permissions";
  return (
    <section className="flex w-full flex-col items-center gap-[18px]">
      <h2 className="text-center text-[26px] leading-[normal] font-semibold tracking-[-0.5px]">
        {permissionsPage ? "Now let it type anywhere" : "Your shortcut"}
      </h2>
      <p className="text-center text-sm text-muted-foreground">
        {!permissionsPage ? "Press it in any app to start dictating."
          : isMacOS ? "Your microphone and one permission. Then Voicetypr works in every app."
          : "Your microphone. Then Voicetypr works in every app."}
      </p>
      {permissionsPage && isMacOS ? <div className="flex w-full max-w-[300px] items-center gap-3">
        <Progress value={(done / checks.length) * 100} aria-label="Setup progress" className="flex-1 [&_[data-slot=progress-indicator]]:bg-sage [&_[data-slot=progress-track]]:h-2" />
        <span className="text-[13px] whitespace-nowrap text-muted-foreground">{done} of {checks.length} permissions</span>
      </div> : null}
      {permissionsPage && (!isMacOS || permissions.microphone.status === "granted") ? <MicrophoneCheck allowed={isMacOS} /> : null}
      {permissionsPage && isMacOS ? (
        <PermissionsStep
          types={permissions.microphone.status === "granted" ? ["accessibility"] : ["microphone", "accessibility"]}
          permissions={permissions}
          checkingPermissions={checkingPermissions}
          isRequestingPermission={isRequestingPermission}
          onCheck={onCheck}
          onRequest={onRequest}
        />
      ) : null}
      {permissionsPage ? null : editor ?? (
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
      {!permissionsPage && !editor ? (
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
      {permissionsPage || !editor ? (
        <div className="flex flex-wrap justify-center gap-[10px]">
          <Button variant="outline" onClick={onBack}>Back</Button>
          {permissionsPage && isMacOS && !accessibilityAllowed ? (
            <Button variant="ghost" disabled={saving} onClick={onSkipAccessibility}>Skip, I’ll paste myself</Button>
          ) : null}
          {!permissionsPage ? (
            <Button variant="ghost" disabled={saving} onClick={onSkip}>Skip for now</Button>
          ) : null}
          <Button disabled={nextDisabled || saving} onClick={onNext}>Continue</Button>
        </div>
      ) : null}
    </section>
  );
}
