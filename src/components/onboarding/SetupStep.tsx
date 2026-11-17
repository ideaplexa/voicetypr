import { KeyCaps } from "@/components/KeyCaps";
import { MicrophoneCheck } from "@/components/onboarding/MicrophoneCheck";
import type { PermissionState } from "@/components/onboarding/onboardingTypes";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";
import { SettingsPaneCard } from "@/components/settings/settings-ui";
import { Button } from "@/components/settings/SettingsButton";
import { isMacOS } from "@/lib/platform";
import { Keyboard } from "lucide-react";
import type { ReactNode } from "react";

export function SetupStep({
  caps,
  holdToTalk,
  heardYou,
  permissions,
  checkingPermissions,
  isRequestingPermission,
  onCheck,
  onRequest,
  editor,
  onChangeShortcut,
  onBack,
  onSkipAccessibility,
  onNext,
  nextDisabled,
  saving,
}: {
  caps: string[];
  holdToTalk: boolean;
  /** A first dictation worked, so the microphone needs no separate check. */
  heardYou: boolean;
  permissions: { microphone: PermissionState; accessibility: PermissionState };
  checkingPermissions: Set<string>;
  isRequestingPermission: string | null;
  onCheck: (type: "microphone" | "accessibility") => void | Promise<void>;
  onRequest: (type: "microphone" | "accessibility") => void | Promise<void>;
  /** The shortcut editor while the user is changing it. */
  editor: ReactNode;
  onChangeShortcut: () => void;
  onBack: () => void;
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
          ? "One permission and your shortcut. Then Voicetypr works in every app."
          : "Your shortcut works in every app. Keep it or pick another."}
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
      {!heardYou && !isMacOS ? <MicrophoneCheck /> : null}
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
        <div className="flex flex-wrap justify-center gap-[10px]">
          <Button variant="outline" onClick={onBack}>Back</Button>
          {isMacOS && !accessibilityAllowed ? (
            <Button variant="ghost" disabled={saving} onClick={onSkipAccessibility}>Skip, I’ll paste myself</Button>
          ) : null}
          <Button disabled={nextDisabled || saving} onClick={onNext}>Continue</Button>
        </div>
      ) : null}
    </section>
  );
}
