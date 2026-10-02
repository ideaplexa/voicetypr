import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Check, Copy, Download, Loader2, ExternalLink } from "lucide-react";
import type { ShareCardStats } from "@/components/shareCardRenderer";

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
      className="relative mx-auto w-full max-w-[600px] overflow-hidden rounded-xl bg-[#121316] ring-1 ring-black/10"
      style={{ aspectRatio: "1200 / 630" }}
    >
      {isLoading ? (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-[#121316]/90 backdrop-blur-sm">
          <div className="flex flex-col items-center gap-2">
            <Loader2 className="size-8 animate-spin text-sage motion-reduce:animate-none" />
            <span className="text-sm text-white/60">Creating your share card…</span>
          </div>
        </div>
      ) : null}
      {imageDataUrl ? (
        <img
          src={imageDataUrl}
          alt={`Share card showing ${stats.totalWords.toLocaleString()} words spoken, ${stats.timeSavedDisplay} saved, and ${stats.streak} day streak`}
          className="block h-auto w-full max-w-full"
        />
      ) : null}
      <canvas
        ref={setCanvas}
        width={1200}
        height={630}
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
  onPost,
}: {
  copied: boolean;
  isCopying: boolean;
  imageDataUrl: string;
  onCopy: () => void;
  onDownload: () => void;
  onPost: () => void;
}) {
  return (
    <div className="flex flex-wrap justify-center gap-2">
      <Button
        onClick={onCopy}
        disabled={isCopying || !imageDataUrl}
        className={cn("min-w-32", copied && "bg-sage text-sage-foreground hover:bg-sage/90")}
      >
        {isCopying ? <Loader2 className="animate-spin" /> : copied ? <Check /> : <Copy />}
        {isCopying ? "Copying…" : copied ? "Copied" : "Copy image"}
      </Button>
      <Button onClick={onDownload} variant="outline" disabled={!imageDataUrl}>
        <Download />
        Save image…
      </Button>
      <Button onClick={onPost} variant="outline" disabled={!imageDataUrl}>
        <ExternalLink />
        Post on X
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
  onPost,
}: {
  isLoading: boolean;
  imageDataUrl: string;
  stats: ShareCardStats;
  setCanvas: (canvas: HTMLCanvasElement | null) => void;
  copied: boolean;
  isCopying: boolean;
  onCopy: () => void;
  onDownload: () => void;
  onPost: () => void;
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
        onPost={onPost}
      />
    </div>
  );
}
