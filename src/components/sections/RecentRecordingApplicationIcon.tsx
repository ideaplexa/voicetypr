import { invoke } from "@tauri-apps/api/core";
import { AppWindow } from "lucide-react";
import { useState, useEffect } from "react";
import { isMacOS } from "@/lib/platform";

const appIconRequests = new Map<string, Promise<string | null>>();

function loadApplicationIcon(processPath: string): Promise<string | null> {
  const cached = appIconRequests.get(processPath);
  if (cached) return cached;

  const request = invoke<string | null>("get_application_icon", { processPath }).catch(() => null);
  appIconRequests.set(processPath, request);
  return request;
}

export function RecentRecordingApplicationIcon({
  appName,
  processPath,
}: {
  appName: string;
  processPath?: string;
}) {
  const [icon, setIcon] = useState<string | null>(null);

  useEffect(() => {
    if (!isMacOS || !processPath) return;

    let cancelled = false;
    void loadApplicationIcon(processPath).then((loadedIcon) => {
      if (!cancelled) setIcon(loadedIcon);
    });
    return () => {
      cancelled = true;
    };
  }, [processPath]);

  return (
    <span
      aria-label={`Application: ${appName}`}
      className="grid size-4 shrink-0 place-items-center"
    >
      {icon ? (
        <img src={icon} alt="" className="size-4 rounded-[4px]" />
      ) : (
        <AppWindow aria-hidden="true" className="size-3.5" />
      )}
    </span>
  );
}
