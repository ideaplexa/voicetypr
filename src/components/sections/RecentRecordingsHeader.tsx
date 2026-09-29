import { PageHeader, InfoButton } from "@/components/settings/settings-ui";
import { Button } from "@/components/settings/SettingsButton";
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
import { isMacOS } from "@/lib/platform";

export interface RecentRecordingsHeaderProps {
  historyLength: number;
  onTranscribeFile?: () => void;
  onExport: () => void;
  onExportText: (format: "txt" | "md") => void;
  onClearAll: () => void;
}

export function RecentRecordingsHeader({
  historyLength,
  onTranscribeFile,
  onExport,
  onExportText,
  onClearAll,
}: RecentRecordingsHeaderProps) {
  return (
    <PageHeader
      title={
        <span className="inline-flex items-center gap-1">
          History
          <Dialog>
            <DialogTrigger render={<InfoButton label="History guide" />} />
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
                  JSON, plain text, or Markdown.
                </p>
              </div>
            </DialogContent>
          </Dialog>
        </span>
      }
      description={`${historyLength.toLocaleString()} dictation${historyLength === 1 ? "" : "s"} · stored only on this ${isMacOS ? "Mac" : "PC"}`}
      action={
        <>
          {onTranscribeFile && (
            <Button
              type="button"
              variant="outline"
              onClick={onTranscribeFile}
              className="h-[35px] rounded-[10px] px-4 text-[13px] font-medium text-muted-foreground"
            >
              Transcribe a file…
            </Button>
          )}
          {historyLength > 0 && (
            <DropdownMenu>
              <DropdownMenuTrigger
                render={
                  <Button
                    type="button"
                    variant="outline"
                    className="h-[35px] rounded-[10px] px-3 text-[13px] text-muted-foreground"
                  />
                }
              >
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
          {historyLength > 0 && (
            <Button
              type="button"
              variant="ghost"
              onClick={onClearAll}
              className="h-[35px] px-2 text-[13px] text-muted-foreground hover:text-destructive"
            >
              Clear all
            </Button>
          )}
        </>
      }
    />
  );
}
