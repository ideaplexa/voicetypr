import { AlertCircle, CheckCircle, Download, HardDrive, Star, Trash2, X, Zap } from "lucide-react";
import { ModelInfo, isLocalModel } from "../types";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Card } from "./ui/card";
import { Progress } from "./ui/progress";
import { Spinner } from "./ui/spinner";
import { cn } from "@/lib/utils";
import { getModelDisplayName } from "@/lib/model-display";
import { createLogger } from "@/lib/logger";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import { useState } from "react";

const log = createLogger("model-card");

interface ModelCardProps {
  name: string;
  model: ModelInfo;
  downloadProgress?: number;
  downloadPhase?: string;
  isVerifying?: boolean;
  downloadError?: string;
  isSelected?: boolean;
  speedModeRecommended?: boolean;
  onDownload: (name: string) => void;
  onSelect: (name: string) => void;
  onDelete?: (name: string) => void;
  onCancelDownload?: (name: string) => void;
  onRepair?: (name: string) => void;
  showSelectButton?: boolean;
}

export const ModelCard = function ModelCard({
  name,
  model,
  downloadProgress,
  downloadPhase,
  downloadError,
  isVerifying = false,
  isSelected = false,
  speedModeRecommended = false,
  onDownload,
  onSelect,
  onDelete,
  onCancelDownload,
  onRepair,
  showSelectButton = true,
}: ModelCardProps) {
  const [warming, setWarming] = useState(false);
  useTauriEvent<string>("parakeet-model-warming", (modelName) => {
    if (modelName === name) setWarming(true);
  });
  useTauriEvent<string>("parakeet-model-warmed", (modelName) => {
    if (modelName === name) setWarming(false);
  });
  if (!isLocalModel(model)) {
    log.debug(`[ModelCard] Skipping non-local model card for ${model.name}`);
    return null;
  }
  const isOnnxParakeet = model.engine === "parakeet" && model.runtime === "onnx";

  const formatSize = () => {
    if (isOnnxParakeet) return `${Math.round(model.size / 1_000_000)} MB`;
    const sizeInMB = model.size / (1024 * 1024);
    return sizeInMB >= 1024 ? `${(sizeInMB / 1024).toFixed(1)} GB` : `${Math.round(sizeInMB)} MB`;
  };

  // Model is usable if fully downloaded and no setup/repair is required.
  const isUsable = model.downloaded && !model.requires_setup;
  const showDownloadError =
    Boolean(downloadError) && !isUsable && downloadProgress === undefined && !isVerifying;
  const downloadLabel = downloadPhase
    ? downloadPhase.charAt(0).toUpperCase() + downloadPhase.slice(1)
    : "Downloading";

  return (
    <Card
      className={cn(
        "group rounded-xl border border-border bg-card p-4 transition-colors",
        isUsable && showSelectButton ? "cursor-pointer" : "",
        isSelected ? "border-sage/50 bg-sage-bg/40" : "hover:border-sage/40 hover:bg-muted/30",
      )}
      onClick={() => isUsable && showSelectButton && onSelect(name)}
    >
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3
              className={cn(
                "truncate text-sm font-semibold tracking-tight",
                isSelected && "text-sage",
              )}
            >
              {getModelDisplayName(name, { [name]: model })}
            </h3>
            {model.recommended && (
              <Badge variant="secondary" className="gap-1 bg-sage-bg text-sage">
                <Star className="size-3 fill-current" aria-label="Recommended model" />
                Recommended
              </Badge>
            )}
            {speedModeRecommended && (
              <Badge variant="outline" className="gap-1 border-sage/40 bg-sage-bg/40 text-sage">
                <Zap className="size-3" />
                Speed mode
              </Badge>
            )}
            {isSelected && (
              <Badge className="gap-1 bg-sage text-sage-foreground">
                <CheckCircle className="size-3" />
                Active
              </Badge>
            )}
          </div>

          <div className="mt-2.5 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs text-muted-foreground">
            <span className="inline-flex items-center gap-1.5">
              <Zap className="size-3.5 text-sage" />
              Speed <span className="font-medium text-foreground">{model.speed_score}</span>
            </span>
            <span className="inline-flex items-center gap-1.5">
              <CheckCircle className="size-3.5 text-sage" />
              Accuracy <span className="font-medium text-foreground">{model.accuracy_score}</span>
            </span>
            <span className="inline-flex items-center gap-1.5">
              <HardDrive className="size-3.5 text-sage" />
              <span className="font-mono text-foreground">{formatSize()}</span>
            </span>
          </div>
          {isOnnxParakeet && (
            <p className="mt-2 text-xs text-muted-foreground">
              Language is detected automatically. About 2× the download size in free disk space is needed temporarily.
            </p>
          )}
          {isOnnxParakeet && warming && (
            <p className="mt-2 inline-flex items-center gap-1.5 text-xs text-muted-foreground"><Spinner className="size-3.5" />Loading model…</p>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          {model.downloaded ? (
            <>
              {onRepair && (
                <Button
                  onClick={(e) => {
                    e.stopPropagation();
                    onRepair(name);
                  }}
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground"
                >
                  Repair
                </Button>
              )}
              {onDelete && (
                <Button
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(name);
                  }}
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground hover:text-destructive"
                >
                  <Trash2 className="size-3.5" />
                  Remove
                </Button>
              )}
            </>
          ) : isVerifying ? (
            <Badge variant="outline" className="gap-1.5 bg-muted/60 text-muted-foreground">
              <Spinner className="size-3.5" />
              Verifying
            </Badge>
          ) : downloadProgress !== undefined ? (
            <>
              {model.engine === "parakeet" && !isOnnxParakeet ? (
                // FluidAudio reports progress only at coarse file/phase
                // boundaries — it jumps 0→25→50→100 and sits between, so a
                // determinate bar looks frozen mid-file. Show an always-animated
                // spinner with the coarse % as a hint: honest "still working".
                <Badge variant="outline" className="gap-1.5 bg-sage-bg text-sage">
                  <Spinner className="size-3.5" />
                  {downloadProgress > 0
                    ? `${downloadLabel} ${Math.round(downloadProgress)}%`
                    : downloadLabel}
                </Badge>
              ) : (
                <div className="flex items-center gap-2.5">
                  <div className="min-w-0">
                    <Progress value={downloadProgress} className="h-1.5 w-24" />
                    {downloadPhase && (
                      <p className="mt-1 max-w-24 truncate text-[10px] text-muted-foreground">
                        {downloadLabel}
                      </p>
                    )}
                  </div>
                  <span className="w-10 text-right text-xs font-medium text-sage">
                    {Math.round(downloadProgress)}%
                  </span>
                </div>
              )}
              {onCancelDownload && (
                <Button
                  onClick={(e) => {
                    e.stopPropagation();
                    onCancelDownload(name);
                  }}
                  variant="ghost"
                  size="icon"
                  className="size-8"
                  aria-label={`Cancel ${getModelDisplayName(name, { [name]: model })} download`}
                >
                  <X className="size-4" />
                </Button>
              )}
            </>
          ) : (
            <Button
              onClick={(e) => {
                e.stopPropagation();
                onDownload(name);
              }}
              variant="default"
              size="sm"
            >
              <Download className="size-4" />
              Download
            </Button>
          )}
        </div>
      </div>
      {showDownloadError ? (
        <div className="mt-3 flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          <AlertCircle className="mt-0.5 size-3.5 shrink-0" />
          <p className="min-w-0">{downloadError}</p>
        </div>
      ) : null}
    </Card>
  );
};
