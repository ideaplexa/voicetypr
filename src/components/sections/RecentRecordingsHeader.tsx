import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Download, HelpCircle, Trash2 } from "lucide-react";
import { isMacOS } from "@/lib/platform";

export interface RecentRecordingsHeaderProps {
  historyLength: number;
  onExport: () => void;
  onExportText: (format: "txt" | "md") => void;
  onClearAll: () => void;
}

export function RecentRecordingsHeader({
  historyLength,
  onExport,
  onExportText,
  onClearAll,
}: RecentRecordingsHeaderProps) {
  return (
    <div className="py-5 pl-2 pr-4">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="text-2xl font-semibold tracking-tight">History</h1>
            <Dialog>
              <DialogTrigger
                render={
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="History guide"
                    className="size-7 rounded-full text-muted-foreground"
                  />
                }
              >
                <HelpCircle className="h-4 w-4" />
              </DialogTrigger>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>History guide</DialogTitle>
                  <DialogDescription>
                    History stores completed transcripts so you can reuse, export, delete, or
                    re-transcribe them.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                  <p>
                    <strong className="text-foreground">Search</strong> filters saved transcripts by
                    text and source metadata.
                  </p>
                  <p>
                    <strong className="text-foreground">Re-transcribe</strong> reruns a saved audio
                    take with your current transcription source. It only appears when the original
                    audio file was saved.
                  </p>
                  <p>
                    <strong className="text-foreground">Export</strong> saves transcript history as
                    JSON for backup or review.
                  </p>
                </div>
              </DialogContent>
            </Dialog>
          </div>
          <p className="mt-0.5 text-sm text-muted-foreground">
            {historyLength > 0
              ? `${historyLength} transcript${historyLength === 1 ? "" : "s"} · stored on this ${isMacOS ? "Mac" : "PC"}`
              : "Your transcripts stay on this device — nothing syncs to a cloud."}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {historyLength > 0 && (
            <DropdownMenu>
              <DropdownMenuTrigger
                render={<Button variant="secondary" size="sm" title="Export transcripts" />}
              >
                <Download className="h-3.5 w-3.5" />
                Export
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem onClick={onExport}>JSON (.json)</DropdownMenuItem>
                <DropdownMenuItem onClick={() => onExportText("txt")}>
                  Plain text (.txt)
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => onExportText("md")}>
                  Markdown (.md)
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
          {historyLength > 5 && (
            <Button
              onClick={onClearAll}
              variant="ghost"
              size="sm"
              className="text-destructive hover:text-destructive"
              title="Clear all transcriptions"
            >
              <Trash2 className="h-3.5 w-3.5" />
              Clear all
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
