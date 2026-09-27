import { Button } from "@/components/ui/button";
import { FileAudio, FileText, Loader2 } from "lucide-react";

export function AudioUploadFileList({
  fileName,
  isProcessing,
  canChange,
  onChangeFile,
  onTranscribe,
}: {
  fileName: string | null;
  isProcessing: boolean;
  canChange: boolean;
  onChangeFile: () => void;
  onTranscribe: () => void;
}) {
  return (
    <>
      <div className="flex items-center justify-between p-3 rounded-lg bg-accent/50">
        <div className="flex items-center gap-3">
          <FileAudio className="h-4 w-4 text-muted-foreground" />
          <span className="text-sm font-medium">{fileName}</span>
        </div>
        {!isProcessing && canChange && (
          <Button size="sm" variant="ghost" onClick={onChangeFile}>
            Change
          </Button>
        )}
      </div>
      <Button onClick={onTranscribe} className="w-full" disabled={isProcessing}>
        {isProcessing ? (
          <>
            <Loader2 className="h-4 w-4 mr-2 animate-spin" />
            Processing...
          </>
        ) : (
          <>
            <FileText className="h-4 w-4 mr-2" />
            Transcribe
          </>
        )}
      </Button>
    </>
  );
}
