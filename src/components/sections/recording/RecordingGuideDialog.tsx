import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { HelpCircle } from "lucide-react";

export function RecordingGuideDialog() {
  return (
    <Dialog>
      <DialogTrigger
        render={
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label="Recording guide"
            className="size-7 rounded-full text-muted-foreground"
          />
        }
      >
        <HelpCircle className="size-4" />
      </DialogTrigger>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Recording guide</DialogTitle>
          <DialogDescription>
            Configure what happens before, during, and after every recording.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3 text-sm leading-6 text-muted-foreground">
          <p>
            <strong className="text-foreground">Capture</strong> controls the primary shortcut and
            microphone.
          </p>
          <p>
            <strong className="text-foreground">Feedback</strong> controls sounds and the recording
            indicator.
          </p>
          <p>
            <strong className="text-foreground">After recording</strong> controls insertion, saved
            audio, and cleanup.
          </p>
        </div>
      </DialogContent>
    </Dialog>
  );
}
