import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Upload, HelpCircle } from "lucide-react";
import { cn } from "@/lib/utils";
import { SettingsPage, SettingsHeader, SettingsCard } from "@/components/settings/settings-ui";
import { AudioUploadDropzone } from "./AudioUploadDropzone";
import { AudioUploadFileList } from "./AudioUploadFileList";
import { AudioUploadResultPanels } from "./AudioUploadResultPanels";
import { useAudioUploadSection } from "./useAudioUploadSection";

export function AudioUploadSection() {
  const {
    copied,
    setCopied,
    isDragging,
    selectedFile,
    status,
    resultText,
    storeError,
    speakerSegments,
    diarizationError,
    diarized,
    clearSelection,
    isProcessing,
    effectiveFileName,
    hasEffectiveSelection,
    activeSourceLabel,
    handleFileSelect,
    handleTranscribe,
    handleCopy,
    handleSaveAs,
    handleReset,
  } = useAudioUploadSection();

  return (
    <SettingsPage>
      <SettingsHeader
        title="Upload"
        description="Transcribe existing audio files, then copy or save the transcript."
        actions={
          <>
            <Badge variant="secondary" className="max-w-[280px] truncate">
              Source: {activeSourceLabel}
            </Badge>
            <Dialog>
              <DialogTrigger
                render={
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    aria-label="Upload guide"
                    className="size-7 rounded-full text-muted-foreground"
                  />
                }
              >
                <HelpCircle className="h-4 w-4" />
              </DialogTrigger>
              <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                  <DialogTitle>Upload guide</DialogTitle>
                  <DialogDescription>
                    Upload uses your currently selected transcription source. Change it in Sources
                    if you want a different model or remote device first.
                  </DialogDescription>
                </DialogHeader>
                <div className="space-y-3 text-sm leading-6 text-muted-foreground">
                  <p>
                    <strong className="text-foreground">Supported files</strong>: WAV, MP3, M4A,
                    FLAC, OGG, MP4, and WebM.
                  </p>
                  <p>
                    <strong className="text-foreground">Video files</strong>: audio is extracted
                    first, then transcribed.
                  </p>
                  <p>
                    <strong className="text-foreground">Long files</strong>: expect longer
                    processing times and higher memory use.
                  </p>
                </div>
              </DialogContent>
            </Dialog>
          </>
        }
      />

      <SettingsCard
        icon={Upload}
        title="Audio file"
        description="Drag and drop an audio or video file, or browse to select one, then transcribe."
      >
        <div
          className={cn(
            "mt-4 overflow-hidden rounded-xl border-2 transition-colors",
            isDragging ? "border-sage bg-sage-bg/40" : "border-transparent",
          )}
        >
          <div className="space-y-4">
            {status !== "done" && (
              <div className="space-y-4">
                {hasEffectiveSelection ? (
                  <AudioUploadFileList
                    fileName={effectiveFileName}
                    isProcessing={isProcessing}
                    canChange={!!selectedFile}
                    onChangeFile={() => {
                      clearSelection();
                      setCopied(false);
                    }}
                    onTranscribe={() => {
                      void handleTranscribe();
                    }}
                  />
                ) : (
                  <AudioUploadDropzone
                    isDragging={isDragging}
                    isProcessing={isProcessing}
                    onSelectFile={() => {
                      void handleFileSelect();
                    }}
                  />
                )}
              </div>
            )}
          </div>
        </div>
      </SettingsCard>

      <AudioUploadResultPanels
        status={status}
        resultText={resultText}
        selectedFile={selectedFile}
        speakerSegments={speakerSegments}
        diarized={diarized}
        diarizationError={diarizationError}
        storeError={storeError}
        copied={copied}
        onCopy={() => {
          void handleCopy();
        }}
        onSaveAs={() => {
          void handleSaveAs();
        }}
        onReset={handleReset}
      />
    </SettingsPage>
  );
}
