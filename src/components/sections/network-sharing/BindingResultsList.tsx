import { Label } from "@/components/ui/label";
import { CheckCircle, Copy, XCircle } from "lucide-react";
import { NO_NETWORK_SENTINEL } from "./sharingUtils";
import type { BindingResult } from "./types";

interface BindingResultsListProps {
  bindingResults: BindingResult[];
  savedPort: string;
  onCopyAddress: (ip: string) => void;
}

export function BindingResultsList({
  bindingResults,
  savedPort,
  onCopyAddress,
}: BindingResultsListProps) {
  const reachableBindings: BindingResult[] = [];
  const failedBindings: BindingResult[] = [];
  for (const result of bindingResults) {
    if (result.ip === "127.0.0.1") continue;
    if (result.success) {
      if (result.ip !== NO_NETWORK_SENTINEL) {
        reachableBindings.push(result);
      }
    } else {
      failedBindings.push(result);
    }
  }

  return (
    <div className="space-y-2">
      <Label className="text-sm font-medium">Connect from another device</Label>
      <div className="space-y-1">
        {reachableBindings.length === 0 ? (
          <div className="rounded-md border border-amber-500/20 bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-400">
            <p className="font-medium">No network address available</p>
            <p className="mt-1 text-xs text-amber-600 dark:text-amber-500">
              Connect this device to Wi-Fi or Ethernet so other Voicetypr apps can reach it.
            </p>
          </div>
        ) : (
          <>
            {reachableBindings.map((result) => (
              <div
                key={`ok:${result.ip}:${result.interface_name ?? ""}`}
                className="flex items-center gap-2"
              >
                <CheckCircle className="h-4 w-4 text-green-500 flex-shrink-0" />
                <div className="flex-1 px-3 py-2 rounded-md bg-background/60 border border-border/60 font-mono text-sm">
                  <span className="font-semibold">
                    {result.ip}:{savedPort}
                  </span>
                  {result.interface_name && (
                    <span className="ml-2 text-xs text-muted-foreground">
                      ({result.interface_name})
                    </span>
                  )}
                </div>
                <button
                  onClick={() => onCopyAddress(result.ip)}
                  className="p-2 rounded-md border border-transparent text-muted-foreground hover:bg-accent hover:text-accent-foreground hover:border-border/50 active:bg-accent/80 active:scale-95 transition-[background-color,color,border-color,transform] duration-150"
                  title="Copy address"
                  aria-label={`Copy address ${result.ip}:${savedPort}`}
                >
                  <Copy className="h-4 w-4" />
                </button>
              </div>
            ))}
            {failedBindings.map((result) => (
              <div
                key={`fail:${result.ip}:${result.interface_name ?? ""}`}
                className="flex items-center gap-2 opacity-50"
                title={result.error || "Could not use this address"}
              >
                <XCircle className="h-4 w-4 text-red-400 flex-shrink-0" />
                <div className="flex-1 px-3 py-2 rounded-md bg-muted/30 border border-border/30 font-mono text-sm text-muted-foreground">
                  <span>
                    {result.ip}:{savedPort}
                  </span>
                  {result.interface_name && (
                    <span className="ml-2 text-xs text-muted-foreground">
                      ({result.interface_name})
                    </span>
                  )}
                  <span className="ml-2 text-xs text-red-400">(could not use this address)</span>
                </div>
              </div>
            ))}
          </>
        )}
      </div>
      {reachableBindings.length > 0 && (
        <p className="text-xs text-muted-foreground">
          Enter one of these addresses in Voicetypr on another device on the same network.
        </p>
      )}
      {failedBindings.length > 0 && (
        <p className="text-xs text-amber-500">
          Some addresses could not be used - hover for details
        </p>
      )}
    </div>
  );
}
