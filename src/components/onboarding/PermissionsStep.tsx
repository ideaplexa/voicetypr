import { LevelBars } from "@/components/onboarding/LevelBars";
import { SecondaryButton, Tag } from "@/components/onboarding/Frame";
import type { PermissionState } from "@/components/onboarding/onboardingTypes";
import { useMicrophoneTest } from "@/components/onboarding/useMicrophoneTest";
import { useSettings } from "@/contexts/SettingsContext";
import { Accessibility, Mic, type LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

type Kind = "microphone" | "accessibility";

function Row({ icon: Icon, title, children, action }: { icon: LucideIcon; title: string; children: ReactNode; action: ReactNode }) {
  return (
    <div className="flex items-center gap-4 border-b border-border px-5 py-[18px] last:border-b-0">
      <span className="flex size-[42px] shrink-0 items-center justify-center rounded-[11px] bg-sage-bg text-sage">
        <Icon aria-hidden="true" className="size-5" />
      </span>
      <div className="flex min-w-0 flex-1 flex-col items-start gap-1.5">
        <h3 className="text-[16.5px] leading-[1.2] font-semibold text-foreground">{title}</h3>
        {children}
      </div>
      {action}
    </div>
  );
}

/** The live meter once the microphone is allowed: levels only, nothing recorded. */
function Listening() {
  const { settings } = useSettings();
  const device = settings?.selected_microphone || undefined;
  const { level, status, visible } = useMicrophoneTest(device);
  const line = !visible || status === "paused" ? "Microphone test paused."
    : status === "opening" ? "Opening microphone…"
    : status === "heard" ? ["We can hear you", device].filter(Boolean).join(" · ")
    : status === "quiet" ? "No input yet — try another microphone in Settings"
    : status === "error" ? "Could not open this microphone"
    : "Say something to check your microphone.";
  return (
    <span className="flex items-center gap-2.5">
      <LevelBars level={level} size="sm" />
      <span role="status" className="text-sm text-muted-foreground">{line}</span>
    </span>
  );
}

/** 05 Let Voicetypr hear you and type for you: the microphone and Accessibility, one row each. */
export function PermissionsStep({
  permissions,
  isRequestingPermission,
  onRequest,
}: {
  permissions: { microphone: PermissionState; accessibility: PermissionState };
  isRequestingPermission: string | null;
  onRequest: (type: Kind) => void | Promise<void>;
}) {
  const action = (type: Kind) => {
    const status = permissions[type].status;
    if (status === "granted") return <Tag>Allowed</Tag>;
    return (
      <SecondaryButton
        disabled={status === "checking" || isRequestingPermission === type}
        onClick={() => void onRequest(type)}
      >
        {status === "checking" ? "Checking…"
          : status !== "not_determined" ? "Open System Settings"
          : type === "microphone" ? "Allow microphone"
          : "Enable Accessibility"}
      </SecondaryButton>
    );
  };
  return (
    <div className="flex flex-col self-stretch overflow-hidden rounded-[14px] border border-border bg-card">
      <Row icon={Mic} title="Microphone" action={action("microphone")}>
        {permissions.microphone.status === "granted"
          ? <Listening />
          : <p className="text-[14.5px] text-muted-foreground">So Voicetypr can hear you.</p>}
      </Row>
      <Row icon={Accessibility} title="Accessibility" action={action("accessibility")}>
        <p className="text-[14.5px] text-muted-foreground">So your words can be typed into any app.</p>
      </Row>
    </div>
  );
}
