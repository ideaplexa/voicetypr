import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { createLogger } from "@/lib/logger";
import { ShareStatsModalBody } from "./ShareStatsModalBody";
import { drawShareCard, type ShareCardStats } from "./shareCardRenderer";

const log = createLogger("share-stats");

interface ShareStatsModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  stats: ShareCardStats;
}

export function ShareStatsModal({ open, onOpenChange, stats }: ShareStatsModalProps) {
  const [canvas, setCanvas] = useState<HTMLCanvasElement | null>(null);
  const [copied, setCopied] = useState(false);
  const [imageDataUrl, setImageDataUrl] = useState("");
  const [isLoading, setIsLoading] = useState(true);
  const [isCopying, setIsCopying] = useState(false);
  // Exactly the fields the card renderer consumes; identity changes only when
  // one of them does, so the draw effect doesn't redraw on unrelated churn.
  const cardStats = useMemo(
    () => ({
      totalTranscriptions: stats.totalTranscriptions,
      totalWords: stats.totalWords,
      timeSavedDisplay: stats.timeSavedDisplay,
    }),
    [stats.totalTranscriptions, stats.totalWords, stats.timeSavedDisplay],
  );

  // Reset transient draw state when the inputs change — adjusted during render.
  const [previousDrawInputs, setPreviousDrawInputs] = useState({
    canvas,
    open,
    cardStats,
  });
  if (
    previousDrawInputs.canvas !== canvas ||
    previousDrawInputs.open !== open ||
    previousDrawInputs.cardStats !== cardStats
  ) {
    const wasOpen = previousDrawInputs.open;
    setPreviousDrawInputs({ canvas, open, cardStats });
    if (open) {
      setImageDataUrl("");
      setIsLoading(true);
    } else if (wasOpen) {
      setIsLoading(true);
      setCopied(false);
    }
  }

  useEffect(() => {
    if (!open || !canvas) return;

    let cancelled = false;

    const drawCard = async () => {
      const dataUrl = await drawShareCard(canvas, cardStats, () => cancelled);
      if (cancelled) return;
      if (dataUrl) {
        setImageDataUrl(dataUrl);
      }
      setIsLoading(false);
    };

    void drawCard();
    return () => {
      cancelled = true;
    };
  }, [canvas, open, cardStats]);

  const copyImageToClipboard = async () => {
    if (!imageDataUrl || isCopying) return;

    setIsCopying(true);
    try {
      await invoke("copy_image_to_clipboard", { imageDataUrl });
      setCopied(true);
      toast.success("Stats image copied to clipboard");
      window.setTimeout(() => setCopied(false), 2000);
    } catch (error) {
      log.error("Failed to copy stats image:", error);
      toast.error("Could not copy the image. Try downloading it instead.");
    } finally {
      setIsCopying(false);
    }
  };

  const downloadImage = async () => {
    if (!imageDataUrl) return;

    try {
      const fileName = `voicetypr-stats-${Date.now()}.png`;
      const filePath = await save({
        defaultPath: fileName,
        filters: [{ name: "Image", extensions: ["png"] }],
      });
      if (filePath) {
        await invoke("save_image_to_file", { imageDataUrl, filePath });
      }
    } catch (error) {
      log.error("Failed to save stats image:", error);
      const link = document.createElement("a");
      link.download = `voicetypr-stats-${Date.now()}.png`;
      link.href = imageDataUrl;
      document.body.appendChild(link);
      link.click();
      link.remove();
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="gap-0 overflow-hidden p-0"
        style={{ width: "calc(100% - 2rem)", maxWidth: "34rem" }}
      >
        <DialogHeader className="border-b border-border/70 px-5 py-3 pr-12 text-left">
          <DialogTitle className="text-base">Share your stats</DialogTitle>
          <DialogDescription>
            A picture of the typing you skipped. Transcript text is never included.
          </DialogDescription>
        </DialogHeader>

        <ShareStatsModalBody
          isLoading={isLoading}
          imageDataUrl={imageDataUrl}
          stats={stats}
          setCanvas={setCanvas}
          copied={copied}
          isCopying={isCopying}
          onCopy={() => {
            void copyImageToClipboard();
          }}
          onDownload={() => {
            void downloadImage();
          }}
        />
      </DialogContent>
    </Dialog>
  );
}
