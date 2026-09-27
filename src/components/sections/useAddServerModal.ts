import { invoke } from "@tauri-apps/api/core";
import React, { useState } from "react";
import { toast } from "sonner";
import { createLogger } from "@/lib/logger";
import type { StatusResponse, TestStatus } from "./AddServerModalFields";

const log = createLogger("server-modal");

export interface SavedConnection {
  id: string;
  host: string;
  port: number;
  password?: string | null;
  has_password?: boolean;
  name: string | null;
  created_at: number;
}

export interface InitialServerValues {
  host: string;
  port: number;
  name?: string | null;
  authRequired?: boolean;
}

export const MIN_REMOTE_SERVER_PORT = 1;
export const MAX_REMOTE_SERVER_PORT = 65535;

export function parseRemoteServerPort(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed || !/^\d+$/.test(trimmed)) return null;
  const port = Number(trimmed);
  if (!Number.isInteger(port) || port < MIN_REMOTE_SERVER_PORT || port > MAX_REMOTE_SERVER_PORT) {
    return null;
  }
  return port;
}

export function useAddServerModal({
  open,
  onOpenChange,
  onServerAdded,
  editServer,
  initialServer,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onServerAdded?: (server: SavedConnection) => void;
  editServer?: SavedConnection | null;
  initialServer?: InitialServerValues | null;
}) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState("47842");
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [testStatus, setTestStatus] = useState<TestStatus>("idle");
  const [showPassword, setShowPassword] = useState(false);
  const [testResult, setTestResult] = useState<StatusResponse | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [localMachineId, setLocalMachineId] = useState<string | null>(null);
  const [isSelfConnection, setIsSelfConnection] = useState(false);
  const [clearSavedPassword, setClearSavedPassword] = useState(false);

  const isEditMode = !!editServer;
  const testRequiresReplacementPassword =
    isEditMode && !!editServer?.has_password && !password && !clearSavedPassword;
  const initialServerRequiresPassword = !isEditMode && !!initialServer?.authRequired;
  const initialPasswordRequirementUnmet =
    initialServerRequiresPassword && (!password.trim() || testStatus !== "success");

  const [appliedFormKey, setAppliedFormKey] = useState<string | null>(null);
  const formKey = !open
    ? "closed"
    : editServer
      ? `edit\u0000${editServer.id}`
      : initialServer
        ? `initial\u0000${initialServer.host}\u0000${initialServer.port}`
        : "blank";
  if (appliedFormKey !== formKey) {
    setAppliedFormKey(formKey);
    if (editServer && open) {
      setHost(editServer.host);
      setPort(editServer.port.toString());
      setPassword(editServer.password || "");
      setName(editServer.name || "");
      setClearSavedPassword(false);
    } else if (initialServer && open) {
      setHost(initialServer.host);
      setPort(initialServer.port.toString());
      setPassword("");
      setName(initialServer.name || "");
      setTestStatus("idle");
      setTestResult(null);
      setTestError(null);
      setClearSavedPassword(false);
    }
  }

  React.useEffect(() => {
    if (open && !localMachineId) {
      invoke<string>("get_local_machine_id")
        .then(setLocalMachineId)
        .catch((err) => log.warn("Failed to get local machine ID:", err));
    }
  }, [open, localMachineId]);

  const resetForm = () => {
    setHost("");
    setPort("47842");
    setPassword("");
    setName("");
    setTestStatus("idle");
    setTestResult(null);
    setTestError(null);
    setIsSelfConnection(false);
    setClearSavedPassword(false);
  };

  const handleClose = () => {
    resetForm();
    onOpenChange(false);
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen) {
      resetForm();
    }
    onOpenChange(nextOpen);
  };

  const resetConnectionTest = () => {
    setTestStatus("idle");
    setTestResult(null);
    setTestError(null);
    setIsSelfConnection(false);
  };

  const updateHost = (value: string) => {
    setHost(value);
    if (initialServerRequiresPassword) resetConnectionTest();
  };

  const updatePort = (value: string) => {
    setPort(value);
    if (initialServerRequiresPassword) resetConnectionTest();
  };

  const updatePassword = (value: string) => {
    setPassword(value);
    if (initialServerRequiresPassword) resetConnectionTest();
  };

  const handleTestConnection = async () => {
    if (!host.trim()) {
      toast.error("Please enter a host address");
      return;
    }

    setTestStatus("testing");
    setTestError(null);
    setTestResult(null);
    setIsSelfConnection(false);

    const validatedPort = parseRemoteServerPort(port);
    if (!validatedPort) {
      toast.error(
        `Enter a valid port between ${MIN_REMOTE_SERVER_PORT} and ${MAX_REMOTE_SERVER_PORT}`,
      );
      setTestStatus("idle");
      return;
    }

    try {
      const data = await invoke<StatusResponse>("test_remote_connection", {
        host: host.trim(),
        port: validatedPort,
        password: password || null,
      });

      if (localMachineId && data.machine_id === localMachineId) {
        setIsSelfConnection(true);
        setTestError("Cannot add your own machine as a remote");
        setTestStatus("error");
        return;
      }

      setTestResult(data);
      setTestStatus("success");

      if (!name.trim() && data.name) {
        setName(data.name);
      }
    } catch (error) {
      log.error("Connection test failed:", error);
      let errorMessage = "Connection failed";

      if (typeof error === "string") {
        if (error.includes("Authentication failed")) {
          errorMessage = "Authentication failed - check password";
        } else if (error.includes("Failed to connect")) {
          errorMessage = "Cannot connect - check host and port";
        } else {
          errorMessage = error;
        }
      }

      setTestError(errorMessage);
      setTestStatus("error");
    }
  };

  const handleSaveServer = async () => {
    if (!host.trim()) {
      toast.error("Please enter a host address");
      return;
    }

    if (isSelfConnection) {
      toast.error("Cannot add your own machine as a remote");
      return;
    }

    if (initialServerRequiresPassword && !password.trim()) {
      toast.error("Enter the password from the sharing device before adding this server");
      return;
    }

    if (initialServerRequiresPassword && testStatus !== "success") {
      toast.error("Test the password-protected server before adding it");
      return;
    }

    const validatedPort = parseRemoteServerPort(port);
    if (!validatedPort) {
      toast.error(
        `Enter a valid port between ${MIN_REMOTE_SERVER_PORT} and ${MAX_REMOTE_SERVER_PORT}`,
      );
      return;
    }

    setSaving(true);
    try {
      let server: SavedConnection;
      if (isEditMode && editServer) {
        const preservePassword = !!editServer.has_password && !password && !clearSavedPassword;
        server = await invoke<SavedConnection>("update_remote_server", {
          serverId: editServer.id,
          host: host.trim(),
          port: validatedPort,
          password: preservePassword ? null : password || null,
          preservePassword,
          name: name.trim() || null,
        });
        toast.success(`"${server.name || server.host}" updated`);
      } else {
        server = await invoke<SavedConnection>("add_remote_server", {
          host: host.trim(),
          port: validatedPort,
          password: password || null,
          name: name.trim() || null,
        });
        toast.success(`"${server.name || server.host}" added`);
      }

      onServerAdded?.(server);
      handleClose();
    } catch (error) {
      log.error(`Failed to ${isEditMode ? "update" : "add"} server:`, error);
      const errorMessage =
        error instanceof Error
          ? error.message
          : `Failed to ${isEditMode ? "update" : "add"} server`;
      toast.error(errorMessage);
    } finally {
      setSaving(false);
    }
  };

  return {
    host,
    port,
    password,
    name,
    setName,
    testStatus,
    showPassword,
    setShowPassword,
    testResult,
    testError,
    saving,
    isSelfConnection,
    clearSavedPassword,
    setClearSavedPassword,
    isEditMode,
    testRequiresReplacementPassword,
    initialServerRequiresPassword,
    initialPasswordRequirementUnmet,
    updateHost,
    updatePort,
    updatePassword,
    handleClose,
    handleOpenChange,
    handleTestConnection,
    handleSaveServer,
  };
}
