import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { createLogger } from "@/lib/logger";

const log = createLogger("overview-tab");

interface SavedConnection {
  id: string;
  host: string;
  port: number;
  name: string | null;
}

export function useActiveRemoteLabel(remoteSelected: boolean): string | null {
  const [activeRemoteLabel, setActiveRemoteLabel] = useState<string | null>(null);

  useEffect(() => {
    if (!remoteSelected) {
      return;
    }

    let cancelled = false;

    const loadActiveRemoteLabel = async () => {
      try {
        const [activeServerId, servers] = await Promise.all([
          invoke<string | null>("get_active_remote_server"),
          invoke<SavedConnection[]>("list_remote_servers"),
        ]);
        if (cancelled) return;

        const activeServer = servers.find((server) => server.id === activeServerId);
        setActiveRemoteLabel(
          activeServer?.name ||
            (activeServer ? `${activeServer.host}:${activeServer.port}` : "Remote Voicetypr"),
        );
      } catch (error) {
        log.error("[OverviewTab] Failed to load active remote Voicetypr:", error);
        if (!cancelled) {
          setActiveRemoteLabel("Remote Voicetypr");
        }
      }
    };

    void loadActiveRemoteLabel();

    return () => {
      cancelled = true;
    };
  }, [remoteSelected]);

  return activeRemoteLabel;
}
