import { MicrophoneCheck } from "@/components/onboarding/MicrophoneCheck";
import type { PermissionState } from "@/components/onboarding/onboardingTypes";
import { PermissionsStep } from "@/components/onboarding/PermissionsStep";
import { FrameHeading, FrameKeys, PrimaryButton, QuietButton, Radio } from "@/components/onboarding/Frame";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { Pencil } from "lucide-react";
import type { ReactNode } from "react";

/** 05 Permissions (Windows: the microphone check) and 06 Your shortcut, each its own page. */
export function SetupStep({
  part,
  caps,
  holdToTalk,
  onHoldToTalkChange,
  permissions,
  isRequestingPermission,
  onRequest,
  editor,
  onChangeShortcut,
  onSkipAccessibility,
  onNext,
  nextDisabled,
  saving,
}: {
  part: "permissions" | "shortcut";
  caps: string[];
  holdToTalk: boolean;
  onHoldToTalkChange: (hold: boolean) => void;
  permissions: { microphone: PermissionState; accessibility: PermissionState };
  isRequestingPermission: string | null;
  onRequest: (type: "microphone" | "accessibility") => void | Promise<void>;
  /** The shortcut editor while the user is changing it. */
  editor: ReactNode;
  onChangeShortcut: () => void;
  onSkipAccessibility: () => void;
  onNext: () => void;
  nextDisabled: boolean;
  saving: boolean;
}) {
  if (part === "permissions" && !isMacOS) {
    return (
      <section className="flex w-[520px] flex-col items-center gap-[30px]">
        <FrameHeading title="Can Voicetypr hear you?" line="Say a few words. This check saves nothing." />
        <MicrophoneCheck />
        <PrimaryButton disabled={nextDisabled || saving} onClick={onNext}>Continue</PrimaryButton>
      </section>
    );
  }
  if (part === "permissions") {
    return (
      <section className="flex w-[620px] flex-col items-center gap-7">
        <FrameHeading
          title="Let Voicetypr hear you and type for you"
          line="The meter only shows sound levels. Nothing is recorded until your first try, two steps from here."
        />
        <PermissionsStep
          permissions={permissions}
          isRequestingPermission={isRequestingPermission}
          onRequest={onRequest}
        />
        <div className="flex flex-col items-center gap-2.5">
          <PrimaryButton disabled={nextDisabled || saving} onClick={onNext}>Continue</PrimaryButton>
          {permissions.accessibility.status === "granted" ? null : (
            <QuietButton disabled={saving} onClick={onSkipAccessibility}>Skip Accessibility, I’ll paste myself</QuietButton>
          )}
        </div>
      </section>
    );
  }
  const mode = (hold: boolean, title: string, line: string) => (
    <button
      type="button"
      role="radio"
      aria-checked={holdToTalk === hold}
      onClick={() => onHoldToTalkChange(hold)}
      className={cn(
        "flex min-w-0 flex-1 items-center gap-3 rounded-[14px] px-[18px] py-4 text-left",
        holdToTalk === hold ? "border-[1.5px] border-sage bg-sage-bg" : "border border-border bg-card hover:bg-accent",
      )}
    >
      <Radio on={holdToTalk === hold} />
      <span className="flex flex-col gap-[3px]">
        <span className="text-[15px] leading-[1.2] font-semibold text-foreground">{title}</span>
        <span className="text-sm leading-[1.2] text-muted-foreground">{line}</span>
      </span>
    </button>
  );
  return (
    <section className="flex w-[560px] flex-col items-center gap-[30px]">
      <FrameHeading title="Your shortcut" line="Press it in any app to start dictating." />
      {editor ?? (
        <>
          <div className="flex flex-col items-center gap-3">
            <FrameKeys caps={caps} size="xl" />
            <QuietButton className="text-sage hover:text-sage" onClick={onChangeShortcut}>
              <Pencil aria-hidden="true" className="size-3.5" />
              Change
            </QuietButton>
          </div>
          <div role="radiogroup" aria-label="How the shortcut works" className="flex gap-3 self-stretch">
            {mode(false, "Press to start, press to stop", "Default")}
            {mode(true, "Hold to talk", "Talk while you hold it")}
          </div>
          <PrimaryButton disabled={nextDisabled || saving} onClick={onNext}>Continue</PrimaryButton>
        </>
      )}
    </section>
  );
}
