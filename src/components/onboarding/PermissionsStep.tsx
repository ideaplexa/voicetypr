import type { PermissionState } from "@/components/onboarding/onboardingTypes";
import { SettingsPaneCard } from "@/components/settings/settings-ui";
import { Button } from "@/components/ui/button";
import { Keyboard, Mic } from "lucide-react";

export function PermissionsStep({
  permissions,
  checkingPermissions,
  isRequestingPermission,
  onCheck,
  onRequest,
}: {
  permissions: { microphone: PermissionState; accessibility: PermissionState };
  checkingPermissions: Set<string>;
  isRequestingPermission: string | null;
  onCheck: (type: "microphone" | "accessibility") => void | Promise<void>;
  onRequest: (type: "microphone" | "accessibility") => void | Promise<void>;
}) {
  return (
    <SettingsPaneCard className="w-full !px-[18px] !py-1">
      {(["microphone", "accessibility"] as const).map((type) => {
        const allowed = permissions[type].status === "granted";
        const Icon = type === "microphone" ? Mic : Keyboard;
        return (
          <div
            key={type}
            className="flex items-center gap-[14px] border-b border-border py-[14px] first:min-h-[92px] last:border-0"
          >
            <span
              className={`flex size-[34px] shrink-0 items-center justify-center rounded-[9px] ${allowed ? "bg-sage-bg" : "bg-muted"}`}
            >
              <Icon className="size-[18px] text-sage" />
            </span>
            <div className="flex-1 space-y-0.5">
              <h3 className="text-sm leading-[normal] font-semibold">
                {type === "microphone" ? "Microphone" : "Accessibility"}
              </h3>
              <p className="text-[12.5px] leading-[normal] text-muted-foreground">
                {type === "microphone"
                  ? "So Voicetypr can hear you."
                  : "So your words can be typed into any app."}
              </p>
            </div>
            {allowed ? (
              <button
                type="button"
                className="flex items-center gap-[5px] rounded-full bg-sage-bg px-[9px] py-[3px] text-[11.5px] font-medium text-muted-foreground"
                disabled={checkingPermissions.has(type)}
                onClick={() => void onCheck(type)}
                aria-label={`Allowed — recheck ${type}`}
              >
                <span className="size-1.5 rounded-full bg-sage" />
                Allowed
              </button>
            ) : (
              <Button
                className="h-auto rounded-[10px] px-4 py-[9px] text-sm"
                disabled={isRequestingPermission === type || checkingPermissions.has(type)}
                onClick={() => void onRequest(type)}
              >
                {permissions[type].status === "checking" ? "Checking…" : "Open System Settings"}
              </Button>
            )}
          </div>
        );
      })}
    </SettingsPaneCard>
  );
}
