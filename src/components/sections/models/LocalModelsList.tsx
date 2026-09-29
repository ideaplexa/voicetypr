import { Button } from "@/components/settings/SettingsButton";
import { Radio } from "@base-ui/react/radio";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Spinner } from "@/components/ui/spinner";
import { getModelDisplayName } from "@/lib/model-display";
import { cn } from "@/lib/utils";
import { Download, Ellipsis, X } from "lucide-react";
import type { LocalModelActions, ModelEntry } from "./types";

interface LocalModelsListProps extends LocalModelActions {
  models: ModelEntry[];
}

export function LocalModelsList({
  models,
  downloadProgress,
  downloadPhases,
  verifyingModels,
  downloadErrors,
  onDownload,
  onDelete,
  onCancelDownload,
  onRepair,
  currentModel,
  activeRemoteServer,
}: LocalModelsListProps) {
  return (
    <div className="divide-y divide-border">
      {models.map(([name, model]) => {
        const displayName = getModelDisplayName(name, { [name]: model });
        const usable = model.downloaded && !model.requires_setup;
        const selected = usable && !activeRemoteServer && currentModel === name;
        const progress = downloadProgress[name];
        const bytes = model.size ?? 0;
        const size =
          bytes >= 1024 ** 3
            ? `${(bytes / 1024 ** 3).toFixed(1)} GB`
            : `${Math.round(bytes / 1024 ** 2)} MB`;
        const detail = `${model.supported_languages?.length ? `${model.supported_languages.length} languages` : model.engine === "parakeet" ? "Multilingual" : "Local transcription"} · ${size}`;
        return (
          <div
            key={name}
            className={cn("flex min-h-[60px] items-center gap-3 px-4 py-3", selected && "bg-sage-bg")}
          >
            <Radio.Root
              value={name}
              aria-label={`Use ${displayName}`}
              disabled={!usable}
              className={cn(
                "size-4 shrink-0 rounded-full border border-border",
                selected && "border-[5px] border-sage",
                usable && "cursor-pointer",
              )}
            />
            <div className="min-w-0 flex-1">
              <p className="truncate text-[13.5px] leading-[normal] font-medium text-foreground">{displayName}</p>
              <p className="mt-0.5 truncate text-xs leading-[normal] text-muted-foreground">{detail}</p>
              {downloadErrors[name] && !usable && progress === undefined ? (
                <p className="text-xs text-destructive">{downloadErrors[name]}</p>
              ) : null}
            </div>
            {selected ? (
              <span className="text-xs font-medium text-sage">In use</span>
            ) : usable ? (
              <span className="text-xs text-muted-foreground">Downloaded</span>
            ) : verifyingModels.has(name) ? (
              <span className="flex items-center gap-1 text-xs text-muted-foreground">
                <Spinner className="size-3" />
                Verifying
              </span>
            ) : progress !== undefined ? (
              <span className="flex items-center gap-1 text-xs text-sage">
                <Spinner className="size-3" />
                {downloadPhases[name] || "Downloading"} {Math.round(progress)}%
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Cancel ${displayName} download`}
                  onClick={() => onCancelDownload(name)}
                >
                  <X className="size-3" />
                </Button>
              </span>
            ) : (
              <Button size="sm" variant="ghost" onClick={() => onDownload(name)}>
                <Download className="size-3.5" />
                Download
              </Button>
            )}
            {model.downloaded && (onRepair || onDelete) ? (
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button variant="ghost" size="icon-sm" aria-label={`${displayName} options`} />
                  }
                >
                  <Ellipsis className="size-4" />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  {onRepair ? (
                    <DropdownMenuItem onClick={() => onRepair(name)}>Repair</DropdownMenuItem>
                  ) : null}
                  {onDelete ? (
                    <DropdownMenuItem onClick={() => onDelete(name)}>Remove</DropdownMenuItem>
                  ) : null}
                </DropdownMenuContent>
              </DropdownMenu>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
