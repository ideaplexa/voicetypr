import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Check, Copy, Download, Loader2 } from "lucide-react";
import type { ShareCardStats } from "./shareCardRenderer";

function ShareStatsPreview({
  isLoading,
  imageDataUrl,
  stats,
  setCanvas,
}: {
  isLoading: boolean;
  imageDataUrl: string;
  stats: ShareCardStats;
  setCanvas: (canvas: HTMLCanvasElement | null) => void;
}) {
  return (
    <div
      className="relative mx-auto w-full max-w-[26rem] overflow-hidden rounded-xl bg-[#161618] ring-1 ring-black/10"
      style={{ aspectRatio: "3 / 2" }}
    >
      {isLoading ? (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-[#161618]/90 backdrop-blur-sm">
          <div className="flex flex-col items-center gap-2">
            <Loader2 className="size-8 animate-spin text-sage" />
            <span className="text-sm text-muted-foreground">Creating your share card…</span>
          </div>
        </div>
      ) : null}
      {imageDataUrl ? (
        <img
          src={imageDataUrl}
          alt={`Share card showing ${stats.totalWords.toLocaleString()} words spoken, ${stats.timeSavedDisplay} saved, and ${stats.totalTranscriptions.toLocaleString()} transcriptions`}
          className="block h-auto w-full max-w-full"
        />
      ) : null}
      <canvas
        ref={setCanvas}
        width={2400}
        height={1600}
        className={cn("block h-auto w-full max-w-full", imageDataUrl && "hidden")}
      />
    </div>
  );
}

function ShareStatsActions({
  copied,
  isCopying,
  imageDataUrl,
  onCopy,
  onDownload,
}: {
  copied: boolean;
  isCopying: boolean;
  imageDataUrl: string;
  onCopy: () => void;
  onDownload: () => void;
}) {
  return (
    <div className="flex justify-center gap-2">
      <Button
        onClick={onCopy}
        disabled={isCopying || !imageDataUrl}
        className={cn("min-w-32", copied && "bg-sage text-sage-foreground hover:bg-sage/90")}
      >
        {isCopying ? <Loader2 className="animate-spin" /> : copied ? <Check /> : <Copy />}
        {isCopying ? "Copying…" : copied ? "Copied" : "Copy image"}
      </Button>
      <Button onClick={onDownload} variant="outline">
        <Download />
        Download
      </Button>
    </div>
  );
}

export function ShareStatsModalBody({
  isLoading,
  imageDataUrl,
  stats,
  setCanvas,
  copied,
  isCopying,
  onCopy,
  onDownload,
}: {
  isLoading: boolean;
  imageDataUrl: string;
  stats: ShareCardStats;
  setCanvas: (canvas: HTMLCanvasElement | null) => void;
  copied: boolean;
  isCopying: boolean;
  onCopy: () => void;
  onDownload: () => void;
}) {
  return (
    <div className="flex min-w-0 flex-col gap-3 p-4">
      <ShareStatsPreview
        isLoading={isLoading}
        imageDataUrl={imageDataUrl}
        stats={stats}
        setCanvas={setCanvas}
      />
      <ShareStatsActions
        copied={copied}
        isCopying={isCopying}
        imageDataUrl={imageDataUrl}
        onCopy={onCopy}
        onDownload={onDownload}
      />
    </div>
  );
}
