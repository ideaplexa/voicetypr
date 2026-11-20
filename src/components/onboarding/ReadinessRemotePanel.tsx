import type { DiscoveredRemoteServer } from "@/components/onboarding/onboardingTypes";
import { isRemoteServerOnline } from "@/components/onboarding/onboardingTypes";
import type { SavedConnection } from "@/components/RemoteServerCard";
import { AddServerModal } from "@/components/sections/AddServerModal";
import { getModelDisplayName } from "@/lib/model-display";
import { cn } from "@/lib/utils";
import { Lock, Plus, Server } from "lucide-react";
import { useEffect, type ReactNode } from "react";

export interface ReadinessRemotePanelProps {
  remoteServers: SavedConnection[];
  discoveredRemoteServers: DiscoveredRemoteServer[];
  activeRemoteServerId: string | null;
  isLoadingRemoteServers: boolean;
  showAddRemoteModal: boolean;
  selectedDiscoveredServer: DiscoveredRemoteServer | null;
  onLoadRemoteServers: () => Promise<void>;
  onOpenAddServer: () => void;
  onAddDiscovered: (server: DiscoveredRemoteServer) => void;
  onSelectRemote: (serverId: string) => void;
  onServerAdded: (server: SavedConnection) => void;
  onAddModalOpenChange: (open: boolean) => void;
}

const rowButton = "flex items-center gap-1.5 rounded-lg border border-border px-3 py-[7px] text-[13.5px] font-medium text-foreground hover:bg-accent disabled:opacity-50";

function Row({ name, about, on, children }: { name: string; about: string; on?: boolean; children: ReactNode }) {
  return (
    <div className={cn("flex h-16 items-center gap-3.5 border-b border-border px-[18px]", on && "bg-sage-bg")}>
      <span className="flex size-[34px] shrink-0 items-center justify-center rounded-[9px] bg-secondary text-foreground">
        <Server aria-hidden="true" className="size-[17px]" />
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="truncate text-[15.5px] leading-[1.2] font-semibold text-foreground">{name}</span>
        <span className="truncate text-sm leading-[1.2] text-muted-foreground">{about}</span>
      </span>
      {children}
    </div>
  );
}

/** 04c Use another computer: computers found on the network, saved ones, and one by address. */
export function ReadinessRemotePanel({
  remoteServers,
  discoveredRemoteServers,
  activeRemoteServerId,
  isLoadingRemoteServers,
  showAddRemoteModal,
  selectedDiscoveredServer,
  onLoadRemoteServers,
  onOpenAddServer,
  onAddDiscovered,
  onSelectRemote,
  onServerAdded,
  onAddModalOpenChange,
}: ReadinessRemotePanelProps) {
  // Computers that turn sharing on while this screen is open show up too.
  useEffect(() => {
    const timer = window.setInterval(() => void onLoadRemoteServers(), 10_000);
    return () => window.clearInterval(timer);
  }, [onLoadRemoteServers]);
  return (
    <div className="flex flex-col gap-[30px] self-stretch">
      <div className="flex flex-col overflow-hidden rounded-[14px] border border-border bg-card">
        {discoveredRemoteServers.map((server) => (
          <Row
            key={`${server.machine_id}:${server.host}:${server.port}`}
            name={server.name || `${server.host}:${server.port}`}
            about={`Found on your network · ${server.auth_required ? "Password required" : getModelDisplayName(server.model)}`}
          >
            <button type="button" className={rowButton} onClick={() => void onAddDiscovered(server)}>
              {server.auth_required ? <Lock aria-hidden="true" className="size-3.5" /> : null}
              {server.auth_required ? "Add with password" : "Use this computer"}
            </button>
          </Row>
        ))}
        {remoteServers.map((server) => {
          const selected = server.id === activeRemoteServerId;
          const online = isRemoteServerOnline(server);
          return (
            <Row
              key={server.id}
              on={selected}
              name={server.name || `${server.host}:${server.port}`}
              about={[online ? "Online" : server.status || "Unknown", server.model ? getModelDisplayName(server.model) : null].filter(Boolean).join(" · ")}
            >
              {selected ? (
                <span className="text-[13.5px] font-medium text-sage">In use</span>
              ) : (
                <button type="button" className={rowButton} disabled={!online} onClick={() => void onSelectRemote(server.id)}>
                  Use
                </button>
              )}
            </Row>
          );
        })}
        {isLoadingRemoteServers && remoteServers.length === 0 && discoveredRemoteServers.length === 0 ? (
          <p className="border-b border-border px-[18px] py-4 text-sm text-muted-foreground">Looking for computers on your network…</p>
        ) : null}
        <button type="button" onClick={onOpenAddServer} className="flex h-14 items-center gap-3.5 px-[18px] text-[15px] font-medium text-sage hover:bg-accent">
          <Plus aria-hidden="true" className="size-4" />
          Add a server by address
        </button>
      </div>
      <p className="text-center text-[14.5px] text-muted-foreground">The other computer needs Voicetypr running with Network sharing on.</p>
      <AddServerModal
        open={showAddRemoteModal}
        onOpenChange={onAddModalOpenChange}
        onServerAdded={onServerAdded}
        initialServer={
          selectedDiscoveredServer
            ? {
                host: selectedDiscoveredServer.host,
                port: selectedDiscoveredServer.port,
                name: selectedDiscoveredServer.name,
                authRequired: selectedDiscoveredServer.auth_required,
              }
            : null
        }
      />
    </div>
  );
}
