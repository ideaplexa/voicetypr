import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Upload } from "lucide-react";

export function AudioUploadDropzone({
  isDragging,
  isProcessing,
  onSelectFile,
}: {
  isDragging: boolean;
  isProcessing: boolean;
  onSelectFile: () => void;
}) {
  return (
    <div
      className={cn(
        "relative rounded-xl border-2 border-dashed p-10 text-center transition-colors",
        isDragging ? "border-sage bg-sage-bg/40" : "border-border/60 hover:border-border",
      )}
    >
      {isDragging ? (
        <div className="space-y-1">
          <Upload className="h-7 w-7 mx-auto text-sage animate-pulse" />
          <p className="text-sm font-medium text-sage">Drop your audio or video file here</p>
          <p className="text-xs text-muted-foreground">WAV, MP3, M4A, FLAC, OGG, MP4, WebM</p>
        </div>
      ) : (
        <div className="space-y-3">
          <div className="space-y-1">
            <Upload className="h-7 w-7 mx-auto text-muted-foreground" />
            <p className="text-sm font-medium">Drag & drop your audio or video file here</p>
            <p className="text-xs text-muted-foreground">or click to browse</p>
          </div>
          <Button
            onClick={onSelectFile}
            variant="outline"
            className="mx-auto"
            disabled={isProcessing}
          >
            <Upload className="h-4 w-4 mr-2" />
            Select File
          </Button>
        </div>
      )}
    </div>
  );
}
