import { useState, type ComponentProps, type ReactNode } from "react";
import { LanguageSelection } from "@/components/LanguageSelection";
import { Button } from "@/components/settings/SettingsButton";
import { ReadinessCloudPanel } from "@/components/onboarding/ReadinessCloudPanel";
import { ReadinessLocalPanel } from "@/components/onboarding/ReadinessLocalPanel";
import { ReadinessRemotePanel } from "@/components/onboarding/ReadinessRemotePanel";
import { SourceStep } from "@/components/onboarding/SourceStep";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

/**
 * "Use something else" on the first try: the on-device model stays the
 * default, and the cloud or another computer use the existing source setup.
 */
export function SourceSheet({
  open,
  showModels,
  sourceType,
  modelSize,
  language,
  local,
  cloud,
  remote,
  onConfirmSource,
  onLanguageChange,
  warning,
  onDone,
}: {
  open: boolean;
  /** Opened from "Choose a different model": the model list starts expanded. */
  showModels: boolean;
  sourceType: SourceType;
  modelSize: string | null;
  language: string;
  local: ComponentProps<typeof ReadinessLocalPanel>;
  cloud: ComponentProps<typeof ReadinessCloudPanel>;
  remote: ComponentProps<typeof ReadinessRemotePanel>;
  onConfirmSource: (source: SourceType) => void;
  onLanguageChange: (value: string) => void;
  /** A model that doesn't support the chosen language asks before downloading. */
  warning: ReactNode;
  onDone: () => void;
}) {
  const [modelsOpen, setModelsOpen] = useState(false);
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) setModelsOpen(showModels);
  }
  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) onDone(); }}>
      <DialogContent className="max-h-[calc(100vh-48px)] overflow-y-auto sm:max-w-[520px]">
        <DialogHeader>
          <DialogTitle>Where should your voice become text?</DialogTitle>
          <DialogDescription>You can change this any time in Settings.</DialogDescription>
        </DialogHeader>
        <div className="flex items-center gap-3 text-sm">
          <span>I speak</span>
          <LanguageSelection value={language} onValueChange={onLanguageChange} />
        </div>
        <SourceStep sourceType={sourceType} onConfirmSource={onConfirmSource} modelSize={modelSize} />
        {sourceType === "local" && (modelsOpen ? <ReadinessLocalPanel {...local} /> : (
          <Button variant="ghost" className="self-start" onClick={() => setModelsOpen(true)}>
            Choose a different model
          </Button>
        ))}
        {sourceType === "cloud" && <ReadinessCloudPanel {...cloud} />}
        {sourceType === "remote" && <ReadinessRemotePanel {...remote} />}
        {warning}
        <DialogFooter>
          <Button onClick={onDone}>Done</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
