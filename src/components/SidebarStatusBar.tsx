import { getVersion } from "@tauri-apps/api/app";
import { Download, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { licenseScreen } from "@/components/navigation";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useUpdateStatus } from "@/hooks/useUpdateStatus";
import { cn } from "@/lib/utils";
import type { LicenseStatus } from "@/types";

export function licenseState(status: LicenseStatus | null, daysLeft: number) {
  if (status?.status === "licensed") return { label: "Pro", color: "text-sage" };
  if (status?.status === "trial") {
    if (daysLeft === 0) return { label: "Trial ends today", color: "text-warn" };
    if (daysLeft === 1) return { label: "Trial · 1 day left", color: "text-warn" };
    return { label: daysLeft > 1 ? `Trial · ${daysLeft} days left` : "Trial", color: "text-sage" };
  }
  return status?.status === "expired"
    ? { label: "Trial ended", color: "text-destructive" }
    : { label: "No License", color: "text-muted-foreground" };
}

const focusRing = "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2";

/** Slim footer: licence on the left, version and update check on the right. Not a nav item. */
export function SidebarStatusBar({
  license,
  collapsed,
  onOpenLicense,
}: {
  license: { label: string; color: string };
  collapsed: boolean;
  onOpenLicense: () => void;
}) {
  const [version, setVersion] = useState("—");
  const [feedback, setFeedback] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const { checking, available, checkNow, promptInstall } = useUpdateStatus();
  useEffect(() => {
    void getVersion()
      .then(setVersion)
      .catch(() => setVersion("—"));
    return () => window.clearTimeout(timer.current);
  }, []);

  const check = async () => {
    const next = await checkNow();
    if (next.available) return;
    setFeedback(
      next.result === "error"
        ? "Check failed"
        : next.result === "store"
          ? "Updated by Store"
          : "Up to date",
    );
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setFeedback(null), 3000);
  };

  const licenseButton = (
    <button
      type="button"
      onClick={onOpenLicense}
      aria-label={`${license.label}. Open License`}
      className={cn(
        "flex min-w-0 items-center gap-1.5 rounded-[6px] text-left text-muted-foreground hover:text-foreground",
        focusRing,
        collapsed ? "size-9 justify-center" : "px-1 py-1",
      )}
    >
      <licenseScreen.icon className={cn("size-[15px] shrink-0", license.color)} />
      <span className={collapsed ? "sr-only" : "min-w-0 truncate text-[12px] leading-[normal] font-medium"}>
        {license.label}
      </span>
    </button>
  );

  const updateButton = available ? (
    <button
      type="button"
      onClick={() => void promptInstall()}
      aria-label={`Update ${available.version} available. Install`}
      title={`Update ${available.version} available`}
      className={cn("relative flex size-6 items-center justify-center rounded-[6px] bg-sage-bg text-sage", focusRing)}
    >
      <Download className="size-[13px]" />
      <span aria-hidden className="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-sage" />
    </button>
  ) : (
    <button
      type="button"
      onClick={() => void check()}
      disabled={checking}
      aria-label={checking ? "Checking for updates" : "Check for updates"}
      title={checking ? "Checking…" : "Check for updates"}
      className={cn("flex size-6 items-center justify-center rounded-[6px] text-muted-foreground hover:text-foreground", focusRing)}
    >
      <RefreshCw className={cn("size-[13px]", checking && "animate-spin")} />
    </button>
  );

  if (collapsed)
    return (
      <div className="flex flex-col items-center gap-0.5">
        <Tooltip>
          <TooltipTrigger render={licenseButton} />
          <TooltipContent role="tooltip" side="right" sideOffset={8}>
            {license.label} · License
          </TooltipContent>
        </Tooltip>
        {available ? updateButton : null}
      </div>
    );

  return (
    <div data-pencil-name="Status bar" className="flex items-center justify-between gap-2 px-1.5 pt-1.5">
      {licenseButton}
      <div className="flex shrink-0 items-center gap-1">
        <span
          role="status"
          className="font-mono text-[11px] leading-[normal] text-muted-foreground"
        >
          {feedback ?? version}
        </span>
        {updateButton}
      </div>
    </div>
  );
}
