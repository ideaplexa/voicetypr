import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/settings/SettingsButton";
import { createLogger } from "@/lib/logger";
import { invoke } from "@tauri-apps/api/core";
import { Check, Copy, Loader2, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { SettingsPaneCard, SettingsPaneRow } from "@/components/settings/settings-ui";

const log = createLogger("cli-tool");

interface CliToolStatus {
  installed: boolean;
  manageable: boolean;
  path: string | null;
  app_version: string;
  command_version: string | null;
  compatible: boolean;
  detail: string | null;
}

const AGENT_PROMPT = `Use Voicetypr for local audio transcription.

When I give you an audio file, run:
voicetypr transcribe --file <file> --json

Read the JSON response, use the transcript for the task I requested, and report any CLI error exactly. Do not upload the audio to another service unless I explicitly ask.`;

/**
 * Surfaces the `voicetypr` command-line tool: install/remove status and a
 * small recipe of example invocations. Installability is driven entirely by
 * the backend's `manageable` flag, so this stays platform-agnostic.
 */
export function AgentCliSection() {
  const [status, setStatus] = useState<CliToolStatus | null>(null);
  const [pending, setPending] = useState<"install" | "repair" | "uninstall" | "refresh" | null>(
    null,
  );
  const [promptCopied, setPromptCopied] = useState(false);

  const refresh = useCallback(async (showError = false) => {
    setPending("refresh");
    try {
      setStatus(await invoke<CliToolStatus>("cli_tool_status"));
    } catch (error) {
      log.error("Failed to read CLI tool status:", error);
      if (showError) toast.error("Failed to refresh CLI health.");
    } finally {
      setPending(null);
    }
  }, []);

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      void refresh();
    }, 0);
    return () => window.clearTimeout(timeoutId);
  }, [refresh]);

  const install = async () => {
    setPending("install");
    try {
      const next = await invoke<CliToolStatus>("install_cli_tool");
      setStatus(next);
      if (next.installed && next.compatible) {
        toast.success("voicetypr CLI installed. Open a new terminal to use it.");
      } else {
        toast.error("Could not install the voicetypr CLI.");
      }
    } catch (error) {
      log.error("Failed to install CLI tool:", error);
      toast.error("Failed to install the voicetypr CLI.");
    } finally {
      setPending(null);
    }
  };

  const repair = async () => {
    setPending("repair");
    try {
      const next = await invoke<CliToolStatus>("repair_cli_tool");
      setStatus(next);
      if (next.compatible) {
        toast.success("voicetypr CLI now matches this app.");
      } else {
        toast.error("Could not repair the voicetypr CLI.");
      }
    } catch (error) {
      log.error("Failed to repair CLI tool:", error);
      toast.error("Failed to repair the voicetypr CLI.");
    } finally {
      setPending(null);
    }
  };

  const uninstall = async () => {
    setPending("uninstall");
    try {
      const next = await invoke<CliToolStatus>("uninstall_cli_tool");
      setStatus(next);
      if (!next.installed) {
        toast.success("voicetypr CLI removed.");
      } else {
        toast.error("Could not remove the voicetypr CLI.");
      }
    } catch (error) {
      log.error("Failed to uninstall CLI tool:", error);
      toast.error("Failed to remove the voicetypr CLI.");
    } finally {
      setPending(null);
    }
  };

  const manageable = status?.manageable ?? false;
  const installed = status?.installed ?? false;
  const compatible = status?.compatible ?? false;
  const busy = pending !== null;

  const copyAgentPrompt = async () => {
    try {
      await navigator.clipboard.writeText(AGENT_PROMPT);
      setPromptCopied(true);
      toast.success("Agent prompt copied");
      window.setTimeout(() => setPromptCopied(false), 2000);
    } catch (error) {
      log.error("Failed to copy agent prompt:", error);
      toast.error("Failed to copy agent prompt");
    }
  };

  return (
    <div className="space-y-3">
      <SettingsPaneCard title="Command line">
        <SettingsPaneRow
          title="voicetypr CLI"
          description={
            status?.path
              ? `Installed at ${status.path} · ${compatible ? "healthy" : "needs repair"}`
              : status === null
                ? "Checking command status…"
                : "Not installed"
          }
          control={
            <Badge variant={compatible ? "secondary" : "outline"} className="text-sm">
              {status === null
                ? "Checking"
                : compatible
                  ? "Installed"
                  : installed
                    ? "Needs attention"
                    : "Not installed"}
            </Badge>
          }
        />
        <SettingsPaneRow
          title={installed ? "Repair or remove" : "Install command line tool"}
          description={
            installed
              ? "Reinstall if the command stops working."
              : "Use Voicetypr from your terminal and AI agents."
          }
          control={
            <div className="flex items-center gap-1">
              {manageable ? (
                installed ? (
                  <>
                    <Button variant="outline" size="sm" onClick={repair} disabled={busy}>
                      {pending === "repair" ? <Loader2 className="animate-spin" /> : null}
                      {compatible ? "Repair" : "Update"}
                    </Button>
                    <Button variant="ghost" size="sm" onClick={uninstall} disabled={busy}>
                      Remove
                    </Button>
                  </>
                ) : (
                  <Button size="sm" onClick={install} disabled={busy}>
                    Install
                  </Button>
                )
              ) : null}
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Refresh CLI health"
                onClick={() => void refresh(true)}
                disabled={busy}
              >
                <RefreshCw className={pending === "refresh" ? "animate-spin" : ""} />
              </Button>
            </div>
          }
        />
        {status?.manageable === false && !status.detail ? (
          <p className="border-t border-border py-2 text-xs text-muted-foreground">
            CLI management is unavailable on this platform.
          </p>
        ) : null}
        {status?.detail ? (
          <p className="border-t border-border py-2 text-xs text-muted-foreground">
            {status.detail}
          </p>
        ) : null}
      </SettingsPaneCard>
      <SettingsPaneCard title="Agents">
        <SettingsPaneRow
          title="Give Voicetypr to your agent"
          description="Copy a short prompt that teaches your agent to use the CLI."
          control={
            <Button type="button" variant="outline" size="sm" onClick={copyAgentPrompt}>
              {promptCopied ? <Check /> : <Copy />}
              {promptCopied ? "Copied" : "Copy agent prompt"}
            </Button>
          }
        />
      </SettingsPaneCard>
    </div>
  );
}
