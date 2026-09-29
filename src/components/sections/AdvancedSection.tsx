import { PermissionErrorBoundary } from "@/components/PermissionErrorBoundary";
import { ResetSection } from "@/components/sections/ResetSection";
import {
  SettingsCard,
  SettingsHeader,
  SettingsPaneHeader,
  SettingRow,
} from "@/components/settings/settings-ui";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { useReadiness } from "@/contexts/ReadinessContext";
import { isMacOS } from "@/lib/platform";
import {
  CheckCircle,
  ChevronDown,
  Download,
  Keyboard,
  HelpCircle,
  Loader2,
  Mic,
  RefreshCw,
  ShieldCheck,
  Type,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";

interface QuickFix {
  id: string;
  title: string;
  icon: LucideIcon;
  issue: string;
  solution: () => string;
}

const QUICK_FIXES: QuickFix[] = [
  {
    id: "recording",
    title: "Recording not working",
    icon: Mic,
    issue: "Voice recording does not start from the shortcut.",
    solution: () =>
      isMacOS
        ? "Check microphone permission in Settings → Troubleshooting. Also confirm a recording device is selected in Recording."
        : "In Windows Settings, allow desktop apps to use the microphone. Also confirm a recording device is selected in Recording.",
  },
  {
    id: "insertion",
    title: "Text not inserting",
    icon: Type,
    issue: "The transcript does not appear at the cursor.",
    solution: () =>
      isMacOS
        ? "Place the cursor in an editable text field. Check Accessibility permission under Settings → Troubleshooting."
        : "Place the cursor in an editable text field, then confirm Auto-paste after transcription is enabled in Settings.",
  },
  {
    id: "hotkey",
    title: "Shortcut not responding",
    icon: Keyboard,
    issue: "The global shortcut does not trigger recording.",
    solution: () =>
      isMacOS
        ? "Open Settings → Troubleshooting and grant Accessibility permission so the global shortcut can work."
        : "Open Shortcuts, and choose another shortcut if the current one is reserved by another app.",
  },
  {
    id: "download",
    title: "Model download stuck",
    icon: Download,
    issue: "A local model download is not progressing.",
    solution: () =>
      "Open Models, cancel the current download, and try again. Check your internet connection before retrying.",
  },
];

export function AdvancedSection({ embedded = false }: { embedded?: boolean } = {}) {
  const [isRequestingPermission, setIsRequestingPermission] = useState<string | null>(null);
  const showAccessibility = isMacOS;
  const [openQuickFixes, setOpenQuickFixes] = useState<string[]>([]);
  const {
    hasAccessibilityPermission,
    hasMicrophonePermission,
    isLoading,
    requestAccessibilityPermission,
    requestMicrophonePermission,
    checkAccessibilityPermission,
    checkMicrophonePermission,
  } = useReadiness();

  const handleRequestPermission = async (type: "microphone" | "accessibility") => {
    setIsRequestingPermission(type);
    try {
      if (type === "microphone") {
        await requestMicrophonePermission();
      } else {
        await requestAccessibilityPermission();
      }
    } finally {
      setIsRequestingPermission(null);
    }
  };

  const refresh = async () => {
    await Promise.all([checkAccessibilityPermission(), checkMicrophonePermission()]);
  };

  const permissionData = [
    {
      type: "microphone" as const,
      icon: Mic,
      title: "Microphone",
      description: embedded ? "Needed to hear you." : "To record your voice for transcription",
      status: hasMicrophonePermission ? "granted" : isLoading ? "checking" : "denied",
    },
    ...(showAccessibility
      ? [
          {
            type: "accessibility" as const,
            icon: Keyboard,
            title: "Accessibility",
            description: embedded
              ? "Needed to paste into other apps."
              : "For global hotkeys to trigger recording",
            status: hasAccessibilityPermission ? "granted" : isLoading ? "checking" : "denied",
          },
        ]
      : []),
    // Automation permission removed for now
    // Can be re-enabled later if needed:
    // {
    //   type: "automation" as const,
    //   icon: TextCursor,
    //   title: "Automation",
    //   description: "To automatically paste transcribed text at cursor",
    //   status: permissions.automation
    // }
  ];

  return (
    <PermissionErrorBoundary>
      <div className={embedded ? "flex flex-col gap-3" : "h-full min-h-0 overflow-auto"}>
        {embedded ? (
          <SettingsPaneHeader
            title="Troubleshooting"
            description="Permissions, quick fixes and reset. Check here before reporting a problem."
          />
        ) : (
          <SettingsHeader
            title={
              <span className="flex items-center gap-2">
                Troubleshooting
                <Dialog>
                  <DialogTrigger
                    render={
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label="Troubleshooting guide"
                        className="size-7 rounded-full text-muted-foreground"
                      />
                    }
                  >
                    <HelpCircle className="h-4 w-4" />
                  </DialogTrigger>
                  <DialogContent className="sm:max-w-lg">
                    <DialogHeader>
                      <DialogTitle>Troubleshooting guide</DialogTitle>
                      <DialogDescription>
                        Troubleshooting covers permissions, troubleshooting, and reset tools.
                      </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                      <p>
                        <strong className="text-foreground">Permissions</strong> refreshes
                        microphone and accessibility access after macOS changes.
                      </p>
                      <p>
                        <strong className="text-foreground">Reset</strong> lets you repeat
                        onboarding or erase app data.
                      </p>
                    </div>
                  </DialogContent>
                </Dialog>
              </span>
            }
            description="Permissions, troubleshooting, and reset tools."
          />
        )}

        {/* Permissions Section - Only show on macOS */}
        {
          <SettingsCard
            compact={embedded}
            icon={embedded ? undefined : ShieldCheck}
            title="Permissions"
            description={
              embedded ? undefined : "System access Voicetypr needs to record and trigger hotkeys."
            }
            action={
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger
                    render={
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => refresh()}
                        disabled={isLoading}
                        className="h-8 px-2"
                      />
                    }
                  >
                    {isLoading ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <RefreshCw className="h-4 w-4" />
                    )}
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Refresh permission status</p>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            }
          >
            {(showAccessibility
              ? permissionData
              : [
                  {
                    type: "microphone" as const,
                    icon: Mic,
                    title: "Microphone",
                    description: "Windows Settings → Privacy → Microphone must allow desktop apps.",
                    status: hasMicrophonePermission ? "granted" : isLoading ? "checking" : "denied",
                  },
                ]
            ).map((perm) => (
              <SettingRow
                className={
                  embedded ? "min-h-[60px] !mt-0 !border-border !py-3 first:!border-t-0" : undefined
                }
                key={perm.type}
                title={
                  <span className="flex items-center gap-2.5">
                    {!embedded ? <perm.icon className="h-4 w-4 text-muted-foreground" /> : null}
                    {perm.title}
                  </span>
                }
                description={perm.description}
              >
                {perm.status === "checking" ? (
                  <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
                ) : perm.status === "granted" ? (
                  <div
                    className={
                      embedded
                        ? "flex items-center gap-1.5 rounded-full bg-sage-bg px-2.5 py-1 text-sage"
                        : "flex items-center gap-1.5 text-green-600"
                    }
                  >
                    <CheckCircle className="h-4 w-4" />
                    <span className="text-sm">Granted</span>
                  </div>
                ) : (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => handleRequestPermission(perm.type)}
                    disabled={isRequestingPermission === perm.type}
                  >
                    {isRequestingPermission === perm.type ? (
                      <Loader2 className="h-3 w-3 animate-spin" />
                    ) : (
                      "Grant"
                    )}
                  </Button>
                )}
              </SettingRow>
            ))}

            {(hasMicrophonePermission === false ||
              (showAccessibility && hasAccessibilityPermission === false)) && (
              <div className="mt-4 border-t border-border pt-4 text-xs text-muted-foreground space-y-1">
                <p className="font-medium">Missing permissions:</p>
                <ul className="list-disc list-inside space-y-0.5 ml-2">
                  {hasMicrophonePermission === false && (
                    <li>Microphone: Required for voice recording</li>
                  )}
                  {showAccessibility && hasAccessibilityPermission === false && (
                    <li>Accessibility: Required for global hotkeys</li>
                  )}
                </ul>
              </div>
            )}
          </SettingsCard>
        }

        <SettingsCard
          compact={embedded}
          icon={embedded ? undefined : Wrench}
          title="Quick fixes"
          description={
            embedded ? undefined : "Common issues you can check yourself before reporting."
          }
        >
          <div className={embedded ? "" : "mt-4 space-y-2"}>
            {QUICK_FIXES.map((fix) => {
              const Icon = fix.icon;
              const isOpen = openQuickFixes.includes(fix.id);

              return (
                <Collapsible
                  key={fix.id}
                  open={isOpen}
                  onOpenChange={() =>
                    setOpenQuickFixes((current) =>
                      current.includes(fix.id)
                        ? current.filter((id) => id !== fix.id)
                        : [...current, fix.id],
                    )
                  }
                >
                  <div
                    className={
                      embedded
                        ? "overflow-hidden border-t border-border bg-card first:border-t-0"
                        : "overflow-hidden rounded-lg border border-border/50 bg-card"
                    }
                  >
                    <CollapsibleTrigger
                      className={
                        embedded
                          ? "flex min-h-[60px] w-full items-center justify-between py-3 text-left transition-colors hover:bg-accent/50"
                          : "flex w-full items-center justify-between px-4 py-3 text-left transition-colors hover:bg-accent/50"
                      }
                    >
                      <span className="flex items-center gap-3">
                        {!embedded ? <Icon className="size-4 text-muted-foreground" /> : null}
                        <span>
                          <span className="block text-sm font-medium text-foreground">
                            {fix.title}
                          </span>
                          {embedded ? (
                            <span className="mt-0.5 block text-xs text-muted-foreground">
                              {fix.id === "recording"
                                ? "Check the microphone and try a test dictation."
                                : fix.id === "insertion"
                                  ? isMacOS
                                    ? "Allow Accessibility, then try again."
                                    : "Check auto-paste, then try again."
                                  : fix.id === "hotkey"
                                    ? "Pick a shortcut no other app uses."
                                    : "Check your internet connection and retry."}
                            </span>
                          ) : null}
                        </span>
                      </span>
                      <ChevronDown
                        className={`size-4 text-muted-foreground transition-transform ${isOpen ? "rotate-180" : ""}`}
                      />
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                      <div className="space-y-3 border-t border-border/50 px-4 pb-4 pt-3">
                        <div>
                          <p className="text-xs font-medium text-muted-foreground">Issue</p>
                          <p className="mt-1 text-sm">{fix.issue}</p>
                        </div>
                        <div>
                          <p className="text-xs font-medium text-muted-foreground">Solution</p>
                          <p className="mt-1 text-sm">{fix.solution()}</p>
                        </div>
                      </div>
                    </CollapsibleContent>
                  </div>
                </Collapsible>
              );
            })}
          </div>
        </SettingsCard>

        <ResetSection pane={embedded} />
      </div>
    </PermissionErrorBoundary>
  );
}
