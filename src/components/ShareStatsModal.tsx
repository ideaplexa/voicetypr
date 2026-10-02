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
import { ShareStatsModalBody } from "@/components/ShareStatsModalBody";
import { drawShareCard, type ShareCardStats } from "@/components/shareCardRenderer";

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
  const [drawError, setDrawError] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const cardStats = useMemo(
    () => ({ ...stats }),
    [stats.totalWords, stats.timeSavedDisplay, stats.streak, stats.pace, stats.range, stats.days],
  );
  const [previousInputs, setPreviousInputs] = useState({ canvas, open, cardStats, attempt });
  if (
    previousInputs.canvas !== canvas ||
    previousInputs.open !== open ||
    previousInputs.cardStats !== cardStats ||
    previousInputs.attempt !== attempt
  ) {
    setPreviousInputs({ canvas, open, cardStats, attempt });
    setImageDataUrl("");
    setIsLoading(true);
    setDrawError(false);
    setCopied(false);
  }
  useEffect(() => {
    if (!open || !canvas) return;
    let cancelled = false;
    void drawShareCard(canvas, cardStats, () => cancelled)
      .then((dataUrl) => {
        if (cancelled) return;
        setImageDataUrl(dataUrl ?? "");
        setDrawError(!dataUrl);
        setIsLoading(false);
      })
      .catch(() => {
        if (!cancelled) {
          setDrawError(true);
          setIsLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [canvas, open, cardStats, attempt]);
  const copyImage = async () => {
    if (!imageDataUrl || isCopying) return;
    setIsCopying(true);
    try {
      await invoke("copy_image_to_clipboard", { imageDataUrl });
      setCopied(true);
      toast.success("Stats image copied to clipboard");
    } catch {
      toast.error("Could not copy the image. Try saving it instead.");
    } finally {
      setIsCopying(false);
    }
  };
  const saveImage = async () => {
    if (!imageDataUrl) return;
    const fileName = `voicetypr-stats-${Date.now()}.png`;
    try {
      const filePath = await save({
        defaultPath: fileName,
        filters: [{ name: "Image", extensions: ["png"] }],
      });
      if (filePath) await invoke("save_image_to_file", { imageDataUrl, filePath });
    } catch {
      const link = document.createElement("a");
      link.download = fileName;
      link.href = imageDataUrl;
      document.body.appendChild(link);
      link.click();
      link.remove();
    }
  };
  const postOnX = async () => {
    const url = new URL("https://x.com/intent/post");
    url.searchParams.set(
      "text",
      `I've dictated ${stats.totalWords.toLocaleString()} words with @voicetypr`,
    );
    url.searchParams.set("url", "https://voicetypr.com");
    try {
      await invoke("plugin:opener|open_url", { url: url.toString(), with: null });
    } catch {
      toast.error("Could not open X. Please try again.");
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="gap-0 overflow-hidden p-0"
        style={{ width: "calc(100% - 2rem)", maxWidth: "40rem" }}
      >
        <DialogHeader className="border-b border-border px-5 py-3 pr-12 text-left">
          <DialogTitle className="text-base">Share your stats</DialogTitle>
          <DialogDescription>
            A picture of the typing you skipped. Transcript text is never included.
          </DialogDescription>
        </DialogHeader>
        <ShareStatsModalBody
          isLoading={isLoading}
          imageDataUrl={imageDataUrl}
          stats={cardStats}
          setCanvas={setCanvas}
          copied={copied}
          isCopying={isCopying}
          onCopy={() => void copyImage()}
          onDownload={() => void saveImage()}
          onPost={() => void postOnX()}
        />
        {drawError && (
          <p role="alert" className="px-4 pb-4 text-sm text-muted-foreground">
            Couldn’t create the image.{" "}
            <button
              className="text-sage underline"
              onClick={() => setAttempt((value) => value + 1)}
            >
              Retry
            </button>
          </p>
        )}
      </DialogContent>
    </Dialog>
  );
}
