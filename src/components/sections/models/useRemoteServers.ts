import { SavedConnection } from "@/components/RemoteServerCard";
import { getErrorMessage } from "@/utils/error";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";
import type { DiscoveredRemoteServer } from "./types";

const log = createLogger("models");

export interface RemoteServersManager {
  remoteServers: SavedConnection[];
  activeRemoteServer: string | null;
  addServerModalOpen: boolean;
  setAddServerModalOpen: (open: boolean) => void;
  editingServer: SavedConnection | null;
  setEditingServer: (server: SavedConnection | null) => void;
  isRefreshingServers: boolean;
  discoveredServers: DiscoveredRemoteServer[];
  selectedDiscoveredServer: DiscoveredRemoteServer | null;
  setSelectedDiscoveredServer: (server: DiscoveredRemoteServer | null) => void;
  isDiscoveringServers: boolean;
  fetchRemoteServers: () => Promise<void>;
  refreshRemoteServers: () => Promise<void>;
  discoverRemoteServers: (notifyEmpty?: boolean) => Promise<void>;
  fetchActiveRemoteServer: () => Promise<void>;
  handleSelectRemoteServer: (serverId: string) => Promise<void>;
  handleDeselectRemoteServer: () => Promise<void>;
  handleRemoveRemoteServer: (serverId: string) => Promise<void>;
  handleServerAdded: (server: SavedConnection) => void;
  handleAddDiscoveredServer: (server: DiscoveredRemoteServer) => Promise<void>;
  handleEditServer: (server: SavedConnection) => void;
  clearActiveRemote: () => Promise<void>;
}

export function useRemoteServers(): RemoteServersManager {
  const [remoteServers, setRemoteServers] = useState<SavedConnection[]>([]);
  const [activeRemoteServer, setActiveRemoteServer] = useState<string | null>(null);
  const [addServerModalOpen, setAddServerModalOpen] = useState(false);
  const [editingServer, setEditingServer] = useState<SavedConnection | null>(null);
  const [isRefreshingServers, setIsRefreshingServers] = useState(false);
  const [discoveredServers, setDiscoveredServers] = useState<DiscoveredRemoteServer[]>([]);
  const [selectedDiscoveredServer, setSelectedDiscoveredServer] =
    useState<DiscoveredRemoteServer | null>(null);
  const [isDiscoveringServers, setIsDiscoveringServers] = useState(false);

  // Quick list fetch (no status checks) - for immediate display
  const fetchRemoteServers = useCallback(async () => {
    try {
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      setRemoteServers(servers);
    } catch (error) {
      log.error("Failed to fetch remote servers:", error);
    }
  }, []);

  // Full refresh with status checks - check each server in parallel for immediate UI updates
  const refreshRemoteServers = useCallback(async () => {
    setIsRefreshingServers(true);
    try {
      // First get the list of servers
      const servers = await invoke<SavedConnection[]>("list_remote_servers");
      setRemoteServers(servers);

      // Check each server in parallel - update UI as each responds
      const checkPromises = servers.map(async (server) => {
        try {
          const updated = await invoke<SavedConnection>("check_remote_server_status", {
            serverId: server.id,
          });
          // Update this specific server in state immediately
          setRemoteServers((prev) => {
            const index = prev.findIndex((s) => s.id === updated.id);
            if (index >= 0) {
              const newList = [...prev];
              newList[index] = updated;
              return newList;
            }
            return prev;
          });
          return updated;
        } catch (error) {
          log.error(`Failed to check server ${server.id}:`, error);
          return server; // Keep existing data on error
        }
      });

      // Wait for all checks to complete
      await Promise.all(checkPromises);
    } catch (error) {
      log.error("Failed to refresh remote servers:", error);
    } finally {
      setIsRefreshingServers(false);
    }
  }, []);

  const discoverRemoteServers = useCallback(
    async (notifyEmpty = false) => {
      setIsDiscoveringServers(true);
      try {
        const discovered = await invoke<DiscoveredRemoteServer[]>("discover_remote_servers", {
          timeoutMs: 1200,
        });
        setDiscoveredServers(discovered);
        if (notifyEmpty && discovered.length === 0) {
          toast.info("No remote Voicetypr devices found. You can still add one manually.");
        }
      } catch (error) {
        log.error("Failed to discover remote Voicetypr devices:", error);
        if (notifyEmpty) {
          toast.error("Failed to scan for remote Voicetypr devices");
        }
      } finally {
        await refreshRemoteServers();
        setIsDiscoveringServers(false);
      }
    },
    [refreshRemoteServers],
  );

  const fetchActiveRemoteServer = useCallback(async () => {
    try {
      const activeId = await invoke<string | null>("get_active_remote_server");
      setActiveRemoteServer(activeId);
    } catch (error) {
      log.error("Failed to fetch active remote server:", error);
    }
  }, []);

  useEffect(() => {
    // On mount: fetch list quickly, then refresh status in background.
    const timeoutId = window.setTimeout(() => {
      void fetchRemoteServers();
      void fetchActiveRemoteServer();
      void discoverRemoteServers();
    }, 0);

    return () => window.clearTimeout(timeoutId);
  }, [fetchRemoteServers, fetchActiveRemoteServer, discoverRemoteServers]);

  // Note: Status updates are handled via the refreshRemoteServers function
  // which calls check_remote_server_status for each server in parallel
  // and updates the UI immediately as each server responds

  // Refresh active remote server when window gains focus (handles tray menu changes)
  useEffect(() => {
    const handleFocus = () => {
      fetchActiveRemoteServer();
      // Refresh server status when user returns to the app
      refreshRemoteServers();
    };

    window.addEventListener("focus", handleFocus);

    // Also listen for Tauri window focus events
    const unlisten = listen("tauri://focus", handleFocus);

    return () => {
      window.removeEventListener("focus", handleFocus);
      unlisten.then((fn) => fn());
    };
  }, [fetchActiveRemoteServer, refreshRemoteServers]);

  const handleSelectRemoteServer = useCallback(
    async (serverId: string) => {
      if (serverId === activeRemoteServer) return;

      try {
        await invoke("set_active_remote_server", { serverId });
        setActiveRemoteServer(serverId);
        toast.success("Remote Voicetypr selected");
      } catch (error) {
        const message = getErrorMessage(error, "Failed to select remote Voicetypr");
        log.error("Failed to set active remote server:", error);
        toast.error(message);
      }
    },
    [activeRemoteServer],
  );

  const handleDeselectRemoteServer = useCallback(async () => {
    try {
      await invoke("set_active_remote_server", { serverId: null });
      setActiveRemoteServer(null);
      toast.success("Remote Voicetypr deselected");
    } catch (error) {
      const message = getErrorMessage(error, "Failed to stop routing to remote Voicetypr");
      log.error("Failed to clear active remote server:", error);
      toast.error(message);
    }
  }, []);

  const handleRemoveRemoteServer = useCallback(
    async (serverId: string) => {
      try {
        if (activeRemoteServer === serverId) {
          await invoke("set_active_remote_server", { serverId: null });
          setActiveRemoteServer(null);
        }

        await invoke("remove_remote_server", { serverId });
        setRemoteServers((prev) => prev.filter((s) => s.id !== serverId));
        toast.success("Remote Voicetypr removed");
      } catch (error) {
        log.error("Failed to remove remote server:", error);
        toast.error("Failed to remove remote Voicetypr");
      }
    },
    [activeRemoteServer],
  );

  const handleServerAdded = useCallback(
    (server: SavedConnection) => {
      setRemoteServers((prev) => {
        // Check if this is an update (server already exists)
        const existingIndex = prev.findIndex((s) => s.id === server.id);
        if (existingIndex >= 0) {
          // Update existing server
          const updated = [...prev];
          updated[existingIndex] = server;
          return updated;
        }
        // Add new server
        return [...prev, server];
      });
      setEditingServer(null);
      // Trigger status refresh for all servers
      refreshRemoteServers();
    },
    [refreshRemoteServers],
  );

  const handleAddDiscoveredServer = useCallback(
    async (server: DiscoveredRemoteServer) => {
      if (server.auth_required) {
        toast.info(
          "This remote Voicetypr requires a password. Enter it to finish adding the server.",
        );
        setSelectedDiscoveredServer(server);
        setEditingServer(null);
        setAddServerModalOpen(true);
        return;
      }

      try {
        const added = await invoke<SavedConnection>("add_remote_server", {
          host: server.host,
          port: server.port,
          password: null,
          name: server.name,
        });
        handleServerAdded(added);
        setDiscoveredServers((prev) =>
          prev.filter(
            (candidate) => !(candidate.host === server.host && candidate.port === server.port),
          ),
        );
        toast.success(`${server.name} added`);
      } catch (error) {
        log.error("Failed to add discovered remote Voicetypr:", error);
        toast.error(error instanceof Error ? error.message : "Failed to add remote Voicetypr");
      }
    },
    [handleServerAdded],
  );

  const handleEditServer = useCallback((server: SavedConnection) => {
    setSelectedDiscoveredServer(null);
    setEditingServer(server);
    setAddServerModalOpen(true);
  }, []);

  const clearActiveRemote = useCallback(async () => {
    try {
      await invoke("set_active_remote_server", { serverId: null });
      setActiveRemoteServer(null);
    } catch (error) {
      log.error("Failed to clear active remote:", error);
    }
  }, []);

  return {
    remoteServers,
    activeRemoteServer,
    addServerModalOpen,
    setAddServerModalOpen,
    editingServer,
    setEditingServer,
    isRefreshingServers,
    discoveredServers,
    selectedDiscoveredServer,
    setSelectedDiscoveredServer,
    isDiscoveringServers,
    fetchRemoteServers,
    refreshRemoteServers,
    discoverRemoteServers,
    fetchActiveRemoteServer,
    handleSelectRemoteServer,
    handleDeselectRemoteServer,
    handleRemoveRemoteServer,
    handleServerAdded,
    handleAddDiscoveredServer,
    handleEditServer,
    clearActiveRemote,
  };
}
