import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { SettingsCard } from "@/components/settings/settings-ui";
import type { SelectedFile, SpeakerSegment } from "@/state/upload";
import { AlertCircle, Check, Copy, Download, FileText } from "lucide-react";

function formatTimestamp(milliseconds: number) {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function AudioUploadResultPanels({
  status,
  resultText,
  selectedFile,
  speakerSegments,
  diarized,
  diarizationError,
  storeError,
  copied,
  onCopy,
  onSaveAs,
  onReset,
}: {
  status: string;
  resultText: string | null;
  selectedFile: SelectedFile | null;
  speakerSegments: SpeakerSegment[];
  diarized: boolean;
  diarizationError: string | null;
  storeError: string | null;
  copied: boolean;
  onCopy: () => void;
  onSaveAs: () => void;
  onReset: () => void;
}) {
  return (
    <>
      {status === "done" && resultText && selectedFile && (
        <SettingsCard
          icon={FileText}
          title="Transcript"
          description="Copy the text or save it to a file. Transcripts are also saved to History."
        >
          <div className="mt-4 space-y-4">
            <div className="p-4 rounded-lg bg-accent/30 space-y-3">
              <div className="flex items-start gap-4">
                <div className="flex-1">
                  <ScrollArea className="h-64">
                    <p className="text-sm leading-relaxed pr-2 whitespace-pre-wrap">{resultText}</p>
                  </ScrollArea>
                </div>
              </div>
              <div className="flex items-center justify-between text-xs text-muted-foreground">
                <span>{selectedFile.name}</span>
                <span>{resultText.split(" ").length} words</span>
              </div>
              {diarized && (
                <p className="text-xs text-muted-foreground/70">
                  Speaker labels via Deepgram / Soniox
                </p>
              )}
            </div>

            {speakerSegments.length > 0 && (
              <div className="rounded-lg border border-border bg-card p-4">
                <div className="mb-3 flex items-center justify-between gap-3">
                  <h3 className="text-sm font-medium">Speaker timeline</h3>
                  <Badge variant="secondary">
                    {new Set(speakerSegments.map((segment) => segment.speaker_id)).size} speakers
                  </Badge>
                </div>
                <ScrollArea className="h-48" aria-label="Speaker timeline segments">
                  <div className="space-y-2 pr-2">
                    {speakerSegments.map((segment) => (
                      <div
                        key={`${segment.speaker_id}-${segment.start_ms}-${segment.end_ms}`}
                        className="flex items-center justify-between gap-3 rounded-md bg-accent/30 px-3 py-2 text-xs"
                      >
                        <span className="font-medium text-foreground">{segment.speaker_id}</span>
                        <span className="font-mono text-muted-foreground">
                          {formatTimestamp(segment.start_ms)}–{formatTimestamp(segment.end_ms)}
                        </span>
                      </div>
                    ))}
                  </div>
                </ScrollArea>
              </div>
            )}

            {diarizationError && (
              <div className="rounded-lg border border-amber-200/50 bg-amber-500/10 p-3 text-xs text-amber-700">
                Speaker timeline unavailable: {diarizationError}
              </div>
            )}

            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-2">
                <Button onClick={onCopy} variant="outline">
                  {copied ? (
                    <>
                      <Check className="h-4 w-4 mr-2 text-green-500" /> Copied
                    </>
                  ) : (
                    <>
                      <Copy className="h-4 w-4 mr-2" /> Copy
                    </>
                  )}
                </Button>
                <Button onClick={onSaveAs} variant="outline">
                  <Download className="h-4 w-4 mr-2" /> Save
                </Button>
              </div>

              <Button onClick={onReset} variant="outline">
                Transcribe Another File
              </Button>
            </div>
          </div>
        </SettingsCard>
      )}

      {status === "error" && storeError && (
        <div className="rounded-lg bg-amber-500/10 border border-amber-200/50 p-4 flex items-start gap-2">
          <AlertCircle className="h-4 w-4 text-amber-600 mt-0.5" />
          <div>
            <p className="text-sm text-amber-700">{storeError}</p>
          </div>
        </div>
      )}

      <section className="rounded-xl border border-border/70 bg-muted/20 px-4 py-3 text-sm">
        <h2 className="font-medium text-foreground">File and processing details</h2>
        <ul className="mt-3 grid gap-2 text-sm leading-relaxed text-muted-foreground sm:grid-cols-2">
          <li>WAV, MP3, M4A, FLAC, OGG, MP4, and WebM are supported.</li>
          <li>Video audio is extracted before transcription.</li>
          <li>Non-WAV media is converted to 16 kHz mono WAV.</li>
          <li>Long media takes more time and memory to process.</li>
        </ul>
        <p className="mt-3 text-xs text-muted-foreground">
          Upload uses the source currently selected in Sources.
        </p>
      </section>
    </>
  );
}
