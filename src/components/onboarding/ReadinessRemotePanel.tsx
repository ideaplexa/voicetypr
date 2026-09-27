import { EmptyState, LoadingState } from "@/components/onboarding/OnboardingChrome";
import type { DiscoveredRemoteServer } from "@/components/onboarding/onboardingTypes";
import { isRemoteServerOnline } from "@/components/onboarding/onboardingTypes";
import type { SavedConnection } from "@/components/RemoteServerCard";
import { AddServerModal } from "@/components/sections/AddServerModal";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Spinner } from "@/components/ui/spinner";
import { getModelDisplayName } from "@/lib/model-display";
import { cn } from "@/lib/utils";
import { Wifi, WifiOff } from "lucide-react";

export interface ReadinessRemotePanelProps {
  remoteServers: SavedConnection[];
  discoveredRemoteServers: DiscoveredRemoteServer[];
  activeRemoteServerId: string | null;
  isLoadingRemoteServers: boolean;
  showAddRemoteModal: boolean;
  selectedDiscoveredServer: DiscoveredRemoteServer | null;
  onLoadRemoteServers: () => void;
  onOpenAddServer: () => void;
  onAddDiscovered: (server: DiscoveredRemoteServer) => void;
  onSelectRemote: (serverId: string) => void;
  onSwitchToLocal: () => void;
  onServerAdded: (server: SavedConnection) => void;
  onAddModalOpenChange: (open: boolean) => void;
}

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
  onSwitchToLocal,
  onServerAdded,
  onAddModalOpenChange,
}: ReadinessRemotePanelProps) {
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-4">
        <div>
          <p className="text-sm font-medium">Saved remote servers</p>
          <p className="text-sm text-muted-foreground">
            Online servers can be selected for transcription.
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            onClick={() => void onLoadRemoteServers()}
            disabled={isLoadingRemoteServers}
          >
            {isLoadingRemoteServers ? <Spinner /> : null}
            Refresh
          </Button>
          <Button onClick={onOpenAddServer}>Add server</Button>
        </div>
      </div>

      <Card className="rounded-2xl border border-border bg-card py-0 shadow-sm">
        <ScrollArea className="h-[320px]">
          <div className="flex flex-col gap-3 p-4">
            {isLoadingRemoteServers && remoteServers.length === 0 ? (
              <LoadingState label="Checking remote servers" />
            ) : null}
            {!isLoadingRemoteServers && remoteServers.length === 0 ? (
              <div className="flex flex-col items-center gap-3">
                <EmptyState
                  title="No remote servers saved"
                  description="Add a Voicetypr server, or set up this device with a local model instead."
                />
                <Button variant="outline" onClick={onSwitchToLocal}>
                  Choose local instead
                </Button>
              </div>
            ) : null}
            {discoveredRemoteServers.map((server) => (
              <Card
                key={`${server.machine_id}:${server.host}:${server.port}`}
                size="sm"
                className="rounded-xl border border-border bg-muted/30"
              >
                <CardHeader>
                  <CardAction>
                    <Badge variant={server.auth_required ? "outline" : "secondary"}>
                      {server.auth_required ? "Password required" : "Found on LAN"}
                    </Badge>
                  </CardAction>
                  <div className="flex items-start gap-3">
                    <div className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                      <Wifi className="size-5" />
                    </div>
                    <div>
                      <CardTitle>{server.name || `${server.host}:${server.port}`}</CardTitle>
                      <CardDescription>
                        {server.host}:{server.port} · {getModelDisplayName(server.model)}
                      </CardDescription>
                    </div>
                  </div>
                </CardHeader>
                <CardFooter className="justify-end">
                  <Button size="sm" onClick={() => void onAddDiscovered(server)}>
                    {server.auth_required ? "Add with password" : "Use this server"}
                  </Button>
                </CardFooter>
              </Card>
            ))}
            {remoteServers.map((server) => {
              const selected = server.id === activeRemoteServerId;
              const online = isRemoteServerOnline(server);
              return (
                <Card
                  key={server.id}
                  size="sm"
                  className={cn(
                    "rounded-xl border border-border bg-muted/30",
                    selected && "border-sage/50 bg-sage-bg/40 ring-1 ring-sage/30",
                  )}
                >
                  <CardHeader>
                    <CardAction>
                      <Badge
                        variant={online ? "secondary" : "outline"}
                        className={cn(online && "bg-sage-bg text-sage")}
                      >
                        {online ? "Online" : server.status || "Unknown"}
                      </Badge>
                    </CardAction>
                    <div className="flex items-start gap-3">
                      <div className="flex size-10 items-center justify-center rounded-xl bg-sage-bg text-sage">
                        {online ? <Wifi className="size-5" /> : <WifiOff className="size-5" />}
                      </div>
                      <div>
                        <CardTitle>{server.name || `${server.host}:${server.port}`}</CardTitle>
                        <CardDescription>
                          {server.host}:{server.port}
                          {server.model ? ` · ${getModelDisplayName(server.model)}` : ""}
                        </CardDescription>
                      </div>
                    </div>
                  </CardHeader>
                  <CardFooter className="justify-end">
                    <Button
                      size="sm"
                      variant={selected ? "default" : "outline"}
                      disabled={!online}
                      onClick={() => void onSelectRemote(server.id)}
                    >
                      {selected ? "Selected" : "Use this server"}
                    </Button>
                  </CardFooter>
                </Card>
              );
            })}
          </div>
        </ScrollArea>
      </Card>

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
