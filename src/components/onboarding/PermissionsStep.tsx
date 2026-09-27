import { OnboardingPanel, StepFooter } from "@/components/onboarding/OnboardingChrome";
import type { PermissionState } from "@/components/onboarding/onboardingTypes";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/utils";
import { CircleCheck, Keyboard, Mic } from "lucide-react";

export function PermissionsStep({
  permissions,
  checkingPermissions,
  isRequestingPermission,
  onCheck,
  onRequest,
  onBack,
  onNext,
  nextDisabled,
}: {
  permissions: {
    microphone: PermissionState;
    accessibility: PermissionState;
  };
  checkingPermissions: Set<string>;
  isRequestingPermission: string | null;
  onCheck: (type: "microphone" | "accessibility") => void | Promise<void>;
  onRequest: (type: "microphone" | "accessibility") => void | Promise<void>;
  onBack: () => void;
  onNext: () => void | Promise<void>;
  nextDisabled: boolean;
}) {
  return (
    <OnboardingPanel
      title="Grant the permissions Voicetypr actually needs"
      description="Microphone starts recording. Accessibility lets the global hotkey work while you are in other apps."
      footer={
        <StepFooter
          onBack={onBack}
          onNext={onNext}
          nextDisabled={nextDisabled}
          nextLabel="Continue"
        />
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        {[
          {
            type: "microphone" as const,
            icon: Mic,
            title: "Microphone",
            desc: "Record your voice for transcription.",
            ...permissions.microphone,
          },
          {
            type: "accessibility" as const,
            icon: Keyboard,
            title: "Accessibility",
            desc: "Use the recording hotkey system-wide.",
            ...permissions.accessibility,
          },
        ].map((perm) => (
          <Card key={perm.type} className="rounded-2xl border border-border bg-card shadow-sm">
            <CardHeader>
              <div className="flex items-start justify-between gap-4">
                <div className="flex gap-3">
                  <div
                    className={cn(
                      "flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage",
                      perm.status === "error" && "bg-destructive/10 text-destructive",
                    )}
                  >
                    <perm.icon className="size-5" />
                  </div>
                  <div>
                    <CardTitle>{perm.title}</CardTitle>
                    <CardDescription>{perm.desc}</CardDescription>
                  </div>
                </div>
                {perm.status === "granted" ? (
                  <Badge variant="secondary" className="gap-1 bg-sage-bg text-sage">
                    <CircleCheck className="size-3" />
                    Granted
                  </Badge>
                ) : null}
              </div>
            </CardHeader>
            <CardFooter className="justify-between gap-3">
              <Button
                variant="outline"
                size="sm"
                onClick={() => void onCheck(perm.type)}
                disabled={checkingPermissions.has(perm.type)}
              >
                {checkingPermissions.has(perm.type) ? <Spinner /> : null}
                Recheck
              </Button>
              <Button
                size="sm"
                onClick={() => void onRequest(perm.type)}
                disabled={isRequestingPermission === perm.type || perm.status === "granted"}
              >
                {isRequestingPermission === perm.type ? <Spinner /> : null}
                Grant access
              </Button>
            </CardFooter>
          </Card>
        ))}
      </div>
    </OnboardingPanel>
  );
}
