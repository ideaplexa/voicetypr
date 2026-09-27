import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Sparkles } from "lucide-react";

interface UpdateAnnouncementDialogProps {
  version: string | null;
  onClose: () => void;
}

function isPolishWorkflowUpdate(version: string | null): boolean {
  return version === "2.0.6" || Boolean(version?.match(/^2\.0\.6-beta\.\d+$/));
}

export function UpdateAnnouncementDialog({ version, onClose }: UpdateAnnouncementDialogProps) {
  return (
    <Dialog open={Boolean(version)} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            Voicetypr Updated
          </DialogTitle>
          <DialogDescription>Successfully updated to version {version}</DialogDescription>
        </DialogHeader>
        {isPolishWorkflowUpdate(version) && (
          <div className="space-y-3 rounded-lg border border-border bg-muted/30 p-4 text-sm">
            <p className="font-medium text-foreground">Polish is now simpler</p>
            <p className="text-muted-foreground">
              Polish now handles punctuation and paragraph breaks automatically. Writing, Notes,
              Message, and Code styles now work through App Rules. Spoken formatting commands have
              been removed.
            </p>
            <p className="text-muted-foreground">
              Your models, AI setup, hotkeys, corrections, Saved Text, and existing App Rules are
              unchanged.
            </p>
          </div>
        )}
        <DialogFooter>
          <Button onClick={onClose}>Dismiss</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
