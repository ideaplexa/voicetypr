import { invoke } from "@tauri-apps/api/core";
import { getModelDisplayName } from "@/lib/model-display";
import { isCloudEngine } from "@/lib/cloudProviders";
import { createLogger } from "@/lib/logger";

const log = createLogger("recordings");

interface SavedConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  model?: string;
}

export interface CurrentTranscriptionSource {
  type: "local" | "cloud" | "remote";
  displayName: string;
  historyModelName: string;
  modelName?: string;
  modelEngine?: string;
  serverId?: string;
}

export async function resolveCurrentTranscriptionSource(
  currentModel?: string,
  currentModelEngine?: string,
): Promise<CurrentTranscriptionSource | null> {
  const activeRemoteServerId = await invoke<string | null>("get_active_remote_server").catch(
    (error) => {
      log.error("Failed to resolve active remote Voicetypr:", error);
      return null;
    },
  );

  if (activeRemoteServerId) {
    let displayBase = "Remote Voicetypr";
    let remoteModel = "";

    try {
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      const server = servers.find((candidate) => candidate.id === activeRemoteServerId);
      if (server) {
        displayBase = server.name || `${server.host}:${server.port}`;
        remoteModel = server.model ?? "";
      }
    } catch (error) {
      log.error("Failed to load active remote Voicetypr label:", error);
    }

    const modelDisplayName = getModelDisplayName(remoteModel) ?? remoteModel;
    return {
      type: "remote",
      serverId: activeRemoteServerId,
      displayName: modelDisplayName ? `${displayBase} - ${modelDisplayName}` : displayBase,
      historyModelName: `Remote: ${displayBase}`,
    };
  }

  const modelName = currentModel?.trim();
  if (!modelName) {
    return null;
  }

  const modelEngine = currentModelEngine ?? "whisper";
  const isCloud = isCloudEngine(modelEngine);
  const displayName = getModelDisplayName(modelName) ?? modelName;

  return {
    type: isCloud ? "cloud" : "local",
    modelName,
    modelEngine,
    displayName,
    historyModelName: displayName,
  };
}
