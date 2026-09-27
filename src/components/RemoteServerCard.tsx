import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ButtonGroup } from "@/components/ui/button-group";
import { Card } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/utils";
import { getModelDisplayName } from "@/lib/model-display";
import { RemoteModelControlSnapshot, SpeechModelEngine } from "@/types";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  CheckCircle2,
  KeyRound,
  Lock,
  Pencil,
  Server,
  Trash2,
  Wifi,
  WifiOff,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";

const log = createLogger("remote-server-card");

// Connection status enum matching backend
export type ConnectionStatus = "Unknown" | "Online" | "Offline" | "AuthFailed" | "SelfConnection";

export interface SavedConnection {
  id: string;
  host: string;
  port: number;
  password?: string | null;
  has_password?: boolean;
  name: string | null;
  created_at: number;
  model?: string | null;
  status?: ConnectionStatus;
  last_checked?: number;
}

export interface StatusResponse {
  status: string;
  version: string;
  model: string;
  name: string;
  machine_id: string;
}

interface RemoteServerCardProps {
  server: SavedConnection;
  isActive: boolean;
  onSelect: (serverId: string) => void;
  onDeselect?: () => void;
  onRemove: (serverId: string) => void;
  onEdit: (server: SavedConnection) => void;
  /** Whether a global refresh is in progress */
  isRefreshing?: boolean;
  /** Refresh saved remote server state after a successful model update */
  onServerUpdated?: () => void | Promise<void>;
}

function modelOptionKey(name: string, engine: SpeechModelEngine) {
  return `${name}::${engine}`;
}

function parseModelOptionKey(value: string): { name: string; engine: SpeechModelEngine } {
  const separator = value.indexOf("::");
  if (separator === -1) {
    return { name: value, engine: "whisper" };
  }

  return {
    name: value.slice(0, separator),
    engine: value.slice(separator + 2) as SpeechModelEngine,
  };
}

function remoteControlErrorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return fallback;
}

function remoteControlUnavailableMessage(error: unknown, fallback: string) {
  const raw = remoteControlErrorMessage(error, fallback);
  if (raw.includes("disabled on this device") || raw.includes("model_control_disabled")) {
    return "The host has not enabled remote model changes.";
  }
  if (raw.includes("requires a sharing password")) {
    return "Add a sharing password on the host so only trusted devices can change the shared model.";
  }
  if (/404|403|unsupported|not found|unavailable/i.test(raw)) {
    return "Remote model changes are not available for this connection.";
  }
  return raw;
}

function lockMessageForStatus(status: ConnectionStatus | undefined, fallback?: string) {
  if (fallback?.trim()) {
    return fallback;
  }

  switch (status) {
    case "AuthFailed":
      return "Password incorrect. Tap edit to update the saved password.";
    case "Offline":
      return "Remote model control is unavailable while the host is offline.";
    default:
      return "Remote model control is unavailable for this connection.";
  }
}

function RemoteTranscriptionModelControl({
  server,
  serverName,
  onUpdated,
  onEdit,
}: {
  server: SavedConnection;
  serverName: string;
  onUpdated?: () => void | Promise<void>;
  onEdit: (server: SavedConnection) => void;
}) {
  const [control, setControl] = useState<RemoteModelControlSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);

  // Clear remote control state when the selected server becomes unavailable.
  const [previousServer, setPreviousServer] = useState({
    id: server.id,
    status: server.status,
  });
  if (previousServer.id !== server.id || previousServer.status !== server.status) {
    setPreviousServer({ id: server.id, status: server.status });
    if (server.status !== "Online") {
      setControl(null);
      setFetchError(null);
    }
  }

  const loadControl = useCallback(async () => {
    setLoading(true);
    setFetchError(null);

    try {
      const result = await invoke<RemoteModelControlSnapshot>("get_remote_transcription_control", {
        serverId: server.id,
      });
      setControl(result);
    } catch (error) {
      log.error("Failed to load remote transcription control:", error);
      setControl(null);
      setFetchError(
        remoteControlUnavailableMessage(
          error,
          "Remote model changes are not available for this connection.",
        ),
      );
    } finally {
      setLoading(false);
    }
  }, [server.id]);

  useEffect(() => {
    if (server.status !== "Online") return;

    const timeoutId = window.setTimeout(() => {
      void loadControl();
    }, 0);
    return () => window.clearTimeout(timeoutId);
  }, [server.id, server.status, loadControl]);

  const selectedValue = useMemo(() => {
    return modelOptionKey(control?.current.id ?? "", control?.current.engine ?? "whisper");
  }, [control?.current.id, control?.current.engine]);

  const handleModelChange = async (value: string) => {
    const { name, engine } = parseModelOptionKey(value);
    if (control?.current.id === name && control.current.engine === engine) {
      return;
    }

    setUpdating(true);
    try {
      await invoke("update_remote_transcription_control", {
        serverId: server.id,
        currentModel: name,
        currentModelEngine: engine,
      });
      toast.success("Remote shared transcription model updated", {
        description: "This device will use the host's updated shared model for remote dictation.",
      });
      await onUpdated?.();
      await loadControl();
    } catch (error) {
      log.error("Failed to update remote transcription control:", error);
      toast.error("Failed to update remote shared transcription model", {
        description: remoteControlUnavailableMessage(error, "The host model could not be changed."),
      });
    } finally {
      setUpdating(false);
    }
  };

  if (server.status === "AuthFailed") {
    return (
      <RemoteModelControlShell>
        <LockedMessage
          icon={KeyRound}
          title={`Transcription model on ${serverName}`}
          message={lockMessageForStatus("AuthFailed")}
          actionLabel="Edit password"
          onAction={() => onEdit(server)}
        />
      </RemoteModelControlShell>
    );
  }

  if (server.status === "Offline") {
    return (
      <RemoteModelControlShell>
        <LockedMessage
          icon={WifiOff}
          title={`Transcription model on ${serverName}`}
          message={lockMessageForStatus("Offline")}
        />
      </RemoteModelControlShell>
    );
  }

  if (server.status !== "Online") {
    return null;
  }

  if (loading && !control) {
    return (
      <RemoteModelControlShell>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Spinner className="size-3" />
          Loading host transcription model...
        </div>
      </RemoteModelControlShell>
    );
  }

  if (fetchError) {
    return (
      <RemoteModelControlShell>
        <LockedMessage
          icon={Lock}
          title={`Transcription model on ${serverName}`}
          message={fetchError}
        />
      </RemoteModelControlShell>
    );
  }

  if (!control) {
    return null;
  }

  const models = control.available;
  const currentLabel =
    control.current.display_name || getModelDisplayName(control.current.id) || control.current.id;
  const hasCurrentOption = models.some(
    (model) => model.id === control.current.id && model.engine === control.current.engine,
  );

  const selectableModels = hasCurrentOption ? models : [control.current, ...models];
  return (
    <RemoteModelControlShell>
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0">
          <p className="text-xs font-medium text-foreground">Transcription model on {serverName}</p>
          <p className="text-[11px] text-muted-foreground">
            Changes the host's shared transcription model, not this device's local model.
          </p>
        </div>
        {selectableModels.length > 0 ? (
          <Select
            items={selectableModels.map((model) => ({
              value: modelOptionKey(model.id, model.engine),
              label: model.display_name || getModelDisplayName(model.id) || model.id,
            }))}
            value={selectedValue}
            onValueChange={(value) => {
              if (value != null) void handleModelChange(value);
            }}
            disabled={updating}
          >
            <SelectTrigger
              className="h-8 w-full sm:w-[220px]"
              aria-label={`Transcription model on ${serverName}`}
              onClick={(event) => event.stopPropagation()}
            >
              <SelectValue placeholder={currentLabel} />
            </SelectTrigger>
            <SelectContent>
              {selectableModels.map((model) => {
                const value = modelOptionKey(model.id, model.engine);
                return (
                  <SelectItem key={value} value={value}>
                    {model.display_name || getModelDisplayName(model.id) || model.id}
                  </SelectItem>
                );
              })}
            </SelectContent>
          </Select>
        ) : (
          <p className="text-xs text-muted-foreground">{currentLabel}</p>
        )}
      </div>
      {updating && (
        <div className="mt-2 flex items-center gap-2 text-[11px] text-muted-foreground">
          <Spinner className="size-3" />
          Updating transcription model on host...
        </div>
      )}
    </RemoteModelControlShell>
  );
}

function RemoteModelControlShell({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="mt-3 border-t border-border/60 pt-3"
      onClick={(event) => event.stopPropagation()}
    >
      {children}
    </div>
  );
}

function LockedMessage({
  icon: Icon,
  title,
  message,
  actionLabel,
  onAction,
}: {
  icon: typeof Lock;
  title: string;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}) {
  return (
    <div className="flex items-start gap-2 rounded-md border border-border/60 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
      <Icon className="mt-0.5 size-3.5 shrink-0" />
      <div className="min-w-0 flex-1">
        <p className="font-medium text-foreground">{title}</p>
        <p className="mt-0.5">{message}</p>
        {actionLabel && onAction && (
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="mt-2 h-7 px-2 text-xs"
            onClick={(event) => {
              event.stopPropagation();
              onAction();
            }}
          >
            {actionLabel}
          </Button>
        )}
      </div>
    </div>
  );
}

export function RemoteServerCard({
  server,
  isActive,
  onSelect,
  onDeselect,
  onRemove,
  onEdit,
  isRefreshing = false,
  onServerUpdated,
}: RemoteServerCardProps) {
  const [removing, setRemoving] = useState(false);

  // Map backend ConnectionStatus to display status
  // Status is now from cached data on server prop
  const getDisplayStatus = ():
    | "unknown"
    | "online"
    | "auth_failed"
    | "offline"
    | "self_connection" => {
    switch (server.status) {
      case "Online":
        return "online";
      case "Offline":
        return "offline";
      case "AuthFailed":
        return "auth_failed";
      case "SelfConnection":
        return "self_connection";
      default:
        return "unknown";
    }
  };
  const status = getDisplayStatus();

  const handleRemove = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setRemoving(true);
    try {
      await onRemove(server.id);
    } finally {
      setRemoving(false);
    }
  };

  const handleEdit = (e: React.MouseEvent) => {
    e.stopPropagation();
    onEdit(server);
  };

  const handleDeselect = (e: React.MouseEvent) => {
    e.stopPropagation();
    onDeselect?.();
  };

  const displayName = server.name || `${server.host}:${server.port}`;
  const modelDisplayName = getModelDisplayName(server.model) ?? server.model;

  // All servers are selectable - status is informational only
  // (Per user request: don't block selection based on status)
  const isSelectable = status !== "self_connection";
  const showRemoteModelControl = status !== "self_connection";

  return (
    <Card
      className={cn(
        "rounded-xl border border-border bg-card p-4 transition-colors",
        isSelectable && !isActive ? "cursor-pointer" : "cursor-default",
        status === "self_connection"
          ? "border-amber-500/30 bg-amber-500/10"
          : isActive
            ? "border-sage/50 bg-sage-bg/40"
            : isSelectable
              ? "hover:border-sage/40 hover:bg-muted/30"
              : "",
      )}
      onClick={() => isSelectable && !isActive && onSelect(server.id)}
    >
      <div className="flex items-center justify-between gap-4">
        <div className="flex min-w-0 flex-1 items-center gap-3">
          <div
            className={cn(
              "flex size-9 shrink-0 items-center justify-center rounded-lg border",
              isActive
                ? "border-sage/30 bg-sage-bg text-sage"
                : status === "online"
                  ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700"
                  : status === "auth_failed" || status === "self_connection"
                    ? "border-amber-500/20 bg-amber-500/10 text-amber-700"
                    : "border-border bg-muted/60 text-muted-foreground",
            )}
          >
            <Server className="size-4" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <h3
                className={cn(
                  "truncate text-sm font-semibold tracking-tight",
                  isActive && "text-sage",
                )}
              >
                {displayName}
              </h3>
              {isActive && (
                <Badge
                  variant="outline"
                  className={cn(
                    "gap-1",
                    status === "online"
                      ? "border-sage/40 bg-sage-bg text-sage"
                      : "border-amber-500/40 bg-amber-500/10 text-amber-800 dark:text-amber-300",
                  )}
                >
                  {status === "online" ? (
                    <>
                      <CheckCircle2 className="size-3" />
                      Routing to {displayName}
                    </>
                  ) : (
                    <>
                      <AlertTriangle className="size-3" />
                      Routing risk:{" "}
                      {status === "auth_failed"
                        ? "Auth failed"
                        : status === "offline"
                          ? "Offline"
                          : "Status unknown"}
                    </>
                  )}
                </Badge>
              )}
            </div>
            <div className="mt-2.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
              {status === "unknown" ? (
                modelDisplayName ? (
                  <>
                    <Wifi className="size-3 text-muted-foreground" />
                    <span>{modelDisplayName}</span>
                    {isRefreshing && <Spinner className="size-3" />}
                  </>
                ) : (
                  <span className="inline-flex items-center gap-1">
                    {isRefreshing ? (
                      <>
                        <Spinner className="size-3" />
                        Checking...
                      </>
                    ) : (
                      "Status unknown"
                    )}
                  </span>
                )
              ) : status === "online" ? (
                <>
                  <Wifi className="size-3 text-emerald-600" />
                  <span className="text-emerald-700 dark:text-emerald-400">Online</span>
                  {modelDisplayName && <span>• {modelDisplayName}</span>}
                  {isRefreshing && <Spinner className="size-3" />}
                </>
              ) : status === "auth_failed" ? (
                <>
                  <KeyRound className="size-3 text-amber-600" />
                  <span className="text-amber-700 dark:text-amber-400">Password incorrect</span>
                  <button
                    type="button"
                    className="text-amber-700 underline underline-offset-2 hover:text-amber-800 dark:text-amber-400 dark:hover:text-amber-300"
                    onClick={handleEdit}
                  >
                    Edit password
                  </button>
                  {isRefreshing && <Spinner className="size-3" />}
                </>
              ) : status === "self_connection" ? (
                <>
                  <AlertTriangle className="size-3 text-amber-600" />
                  <span className="text-amber-700 dark:text-amber-400">This Machine</span>
                  <span>• This is the same device</span>
                </>
              ) : (
                <>
                  <WifiOff className="size-3 text-destructive" />
                  <span className="text-destructive">Offline</span>
                  {isRefreshing && <Spinner className="size-3" />}
                </>
              )}
            </div>
          </div>
        </div>

        <div className="flex shrink-0 items-center gap-1.5">
          {isActive && onDeselect && (
            <Button size="sm" variant="outline" onClick={handleDeselect}>
              Stop routing
            </Button>
          )}

          <ButtonGroup>
            <Button
              size="sm"
              variant="ghost"
              className="size-8 p-0 text-muted-foreground hover:text-foreground"
              onClick={handleEdit}
              title="Edit server"
              aria-label={`Edit ${displayName}`}
            >
              <Pencil className="size-4" />
            </Button>
            <Button
              size="sm"
              variant="ghost"
              className="size-8 p-0 text-muted-foreground hover:text-destructive"
              onClick={handleRemove}
              disabled={removing}
              title="Remove server"
              aria-label={`Remove ${displayName}`}
            >
              {removing ? <Spinner className="size-4" /> : <Trash2 className="size-4" />}
            </Button>
          </ButtonGroup>
        </div>
      </div>

      {showRemoteModelControl && (
        <RemoteTranscriptionModelControl
          server={server}
          serverName={displayName}
          onUpdated={onServerUpdated}
          onEdit={onEdit}
        />
      )}
    </Card>
  );
}
