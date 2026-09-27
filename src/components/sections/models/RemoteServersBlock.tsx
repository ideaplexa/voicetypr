import { AddServerModal } from "../AddServerModal";
import { RemoteServerCard } from "@/components/RemoteServerCard";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { SettingsCard } from "@/components/settings/settings-ui";
import { Spinner } from "@/components/ui/spinner";
import { getModelDisplayName, humanizeModelId } from "@/lib/model-display";
import { Plus, Server } from "lucide-react";
import type { RemoteServersManager } from "./useRemoteServers";

interface RemoteServersBlockProps {
  remotes: RemoteServersManager;
  visible: boolean;
}

export function RemoteServersBlock({ remotes, visible }: RemoteServersBlockProps) {
  const {
    remoteServers,
    activeRemoteServer,
    discoveredServers,
    isDiscoveringServers,
    isRefreshingServers,
    addServerModalOpen,
    setAddServerModalOpen,
    editingServer,
    setEditingServer,
    selectedDiscoveredServer,
    setSelectedDiscoveredServer,
    discoverRemoteServers,
    handleSelectRemoteServer,
    handleDeselectRemoteServer,
    handleRemoveRemoteServer,
    handleEditServer,
    handleServerAdded,
    handleAddDiscoveredServer,
    refreshRemoteServers,
  } = remotes;
  return (
    <>
      {visible && (
        <SettingsCard
          icon={Server}
          title={`Remote Voicetypr (${remoteServers.length})`}
          description="Use another Voicetypr device on your network without copying audio to the cloud."
          action={
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                variant="outline"
                onClick={() => void discoverRemoteServers(true)}
                disabled={isDiscoveringServers}
              >
                {isDiscoveringServers ? <Spinner className="size-4" /> : null}
                Scan LAN
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={() => {
                  setSelectedDiscoveredServer(null);
                  setAddServerModalOpen(true);
                }}
              >
                <Plus className="size-4" />
                Add manually
              </Button>
            </div>
          }
        >
          <div className="mt-4 space-y-3">
            {discoveredServers.length > 0 && (
              <div className="grid gap-3">
                {discoveredServers.map((server) => {
                  const alreadySaved = remoteServers.some(
                    (saved) => saved.host === server.host && saved.port === server.port,
                  );

                  if (alreadySaved) return null;

                  return (
                    <Card
                      key={`${server.machine_id}:${server.host}:${server.port}`}
                      className="rounded-xl border border-border bg-card p-4 transition-colors hover:border-sage/40 hover:bg-muted/30"
                    >
                      <div className="flex items-center justify-between gap-4">
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <Server className="size-4 shrink-0 text-sage" />
                            <h3 className="truncate text-sm font-semibold tracking-tight">
                              {server.name}
                            </h3>
                            <Badge variant={server.auth_required ? "outline" : "secondary"}>
                              {server.auth_required ? "Password required" : "Found on LAN"}
                            </Badge>
                          </div>
                          <p className="mt-2.5 flex flex-wrap items-center gap-x-3 text-xs text-muted-foreground">
                            <span>
                              {server.host}:{server.port}
                            </span>
                            <span>·</span>
                            <span className="truncate">
                              {getModelDisplayName(server.model) ?? humanizeModelId(server.model)}
                            </span>
                          </p>
                        </div>
                        <Button
                          size="sm"
                          className="shrink-0"
                          onClick={() => void handleAddDiscoveredServer(server)}
                        >
                          {server.auth_required ? "Add with password" : "Add"}
                        </Button>
                      </div>
                    </Card>
                  );
                })}
              </div>
            )}
            {remoteServers.length > 0 ? (
              <div className="grid gap-3">
                {remoteServers.map((server) => (
                  <RemoteServerCard
                    key={server.id}
                    server={server}
                    isActive={activeRemoteServer === server.id}
                    onSelect={handleSelectRemoteServer}
                    onDeselect={handleDeselectRemoteServer}
                    onRemove={handleRemoveRemoteServer}
                    onEdit={handleEditServer}
                    isRefreshing={isRefreshingServers}
                    onServerUpdated={refreshRemoteServers}
                  />
                ))}
              </div>
            ) : (
              <Empty className="border border-border/60 bg-card/70 py-8">
                <EmptyHeader>
                  <EmptyMedia variant="icon">
                    <Server className="size-5" />
                  </EmptyMedia>
                  <EmptyTitle>No remote Voicetyprs configured</EmptyTitle>
                  <EmptyDescription>
                    Connect another Voicetypr device to use its local model from this machine.
                  </EmptyDescription>
                </EmptyHeader>
              </Empty>
            )}
          </div>
        </SettingsCard>
      )}
      <AddServerModal
        open={addServerModalOpen}
        onOpenChange={(open) => {
          setAddServerModalOpen(open);
          if (!open) {
            setEditingServer(null);
            setSelectedDiscoveredServer(null);
          }
        }}
        onServerAdded={handleServerAdded}
        editServer={editingServer}
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
    </>
  );
}
