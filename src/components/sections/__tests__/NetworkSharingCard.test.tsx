import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ReactNode } from "react";

// Mock Tauri invoke
const mockInvoke = vi.fn();
const eventListeners = new Map<string, Array<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

// Mock Tauri events
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, callback: (e: { payload: unknown }) => void) => {
    const listeners = eventListeners.get(event) ?? [];
    listeners.push(callback);
    eventListeners.set(event, listeners);

    return Promise.resolve(() => {
      const current = eventListeners.get(event) ?? [];
      eventListeners.set(
        event,
        current.filter((listener) => listener !== callback),
      );
    });
  }),
}));

// Mock sonner toast - using inline object to avoid hoisting issues
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
  },
}));

// Import toast after mocking to get the mocked version
import { toast } from "sonner";

import { NetworkSharingCard } from "../NetworkSharingCard";
import { SettingsProvider } from "@/contexts/SettingsContext";

// Wrapper component that provides SettingsContext
function TestWrapper({ children }: { children: ReactNode }) {
  return <SettingsProvider>{children}</SettingsProvider>;
}

// Helper to render with providers
function renderWithProviders(ui: React.ReactElement) {
  return render(ui, { wrapper: TestWrapper });
}

function sharingStatus(overrides: Record<string, unknown> = {}) {
  return {
    enabled: true,
    port: 47842,
    model_name: "large-v3-turbo",
    server_name: "My-PC",
    active_connections: 0,
    password_configured: true,
    binding_results: [],
    allow_model_control: false,
    ...overrides,
  };
}

function shareableModel(overrides: Record<string, unknown> = {}) {
  return {
    name: "large-v3-turbo",
    display_name: "Large v3 Turbo",
    downloaded: true,
    engine: "whisper",
    kind: "local",
    ...overrides,
  };
}

describe("NetworkSharingCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventListeners.clear();
  });

  describe("when no model is downloaded", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: null,
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                shareableModel({ downloaded: false }),
                shareableModel({
                  name: "base.en",
                  display_name: "Base (English)",
                  downloaded: false,
                }),
              ],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows warning when no shareable local model is available", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("No shareable local model")).toBeInTheDocument();
      });

      expect(
        screen.getByText(/Remote sharing requires a downloaded Whisper or Parakeet model/),
      ).toBeInTheDocument();
    });

    it("disables the toggle switch when no shareable local model is available", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        const toggle = screen.getByRole("switch");
        expect(toggle).toHaveAttribute("aria-disabled", "true");
      });
    });
  });

  describe("when a model is downloaded", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)", "10.0.0.5 (WiFi)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                shareableModel(),
                shareableModel({
                  name: "base.en",
                  display_name: "Base (English)",
                  downloaded: false,
                }),
              ],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows which model will be shared", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText(/Large v3 Turbo/)).toBeInTheDocument();
      });

      expect(screen.getByText(/is only available on this device/)).toBeInTheDocument();
    });

    it("enables the toggle switch when a model is available", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        const toggle = screen.getByRole("switch");
        expect(toggle).not.toHaveAttribute("aria-disabled", "true");
      });
    });

    it("does not show the no shareable model warning", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.queryByText("No model downloaded")).not.toBeInTheDocument();
      });
    });
  });

  describe("when sharing is enabled", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows ready remote transcription status", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Ready for remote transcription")).toBeInTheDocument();
      });
    });

    it("shows the model being shared with friendly name", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Model: Large v3 Turbo")).toBeInTheDocument();
      });
    });

    it("displays IP addresses with port", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText(/192\.168\.1\.100:47842/)).toBeInTheDocument();
      });
    });
  });

  describe("UI messaging", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows clear description about remote transcription", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(
          screen.getByText("Use this device's transcription from another Voicetypr app"),
        ).toBeInTheDocument();
      });
    });
  });

  describe("when model selection changes while sharing", () => {
    let currentSettings: {
      current_model: string;
      current_model_engine?: string;
      auto_insert: boolean;
      launch_at_startup: boolean;
    };
    let sharingStatus: {
      enabled: boolean;
      port: number;
      model_name: string;
      server_name: string;
      active_connections: number;
    };

    beforeEach(() => {
      currentSettings = {
        current_model: "large-v3-turbo",
        current_model_engine: "whisper",
        auto_insert: true,
        launch_at_startup: false,
      };
      sharingStatus = {
        enabled: true,
        port: 47842,
        model_name: "large-v3-turbo",
        server_name: "My-PC",
        active_connections: 0,
      };

      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve(currentSettings);
          case "get_sharing_status":
            return Promise.resolve(sharingStatus);
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                shareableModel(),
                shareableModel({ name: "base.en", display_name: "Base (English)" }),
              ],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("does not restart sharing just because refreshed shared model differs", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Ready for remote transcription")).toBeInTheDocument();
      });

      sharingStatus = {
        ...sharingStatus,
        model_name: "base.en",
      };
      for (const listener of eventListeners.get("sharing-status-changed") ?? []) {
        listener({ payload: null });
      }

      await waitFor(() => {
        expect(screen.getByText("Model: Base (English)")).toBeInTheDocument();
      });
      expect(mockInvoke).not.toHaveBeenCalledWith("stop_sharing");
      expect(mockInvoke).not.toHaveBeenCalledWith("start_sharing", expect.any(Object));
    });

    it("automatically restarts sharing after the local model changes", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Ready for remote transcription")).toBeInTheDocument();
      });

      currentSettings = {
        ...currentSettings,
        current_model: "base.en",
      };
      for (const listener of eventListeners.get("model-changed") ?? []) {
        listener({ payload: null });
      }

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("stop_sharing");
        expect(mockInvoke).toHaveBeenCalledWith("start_sharing", expect.any(Object));
      });
    });

    it("does not show manual Update button", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Ready for remote transcription")).toBeInTheDocument();
      });

      expect(screen.queryByRole("button", { name: /Update/i })).not.toBeInTheDocument();
    });
  });

  describe("when using a remote server", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve("remote-server-1");
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows warning when using remote Voicetypr", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Using remote Voicetypr")).toBeInTheDocument();
      });

      expect(
        screen.getByText(/Remote transcription is unavailable while using another Voicetypr/),
      ).toBeInTheDocument();
    });

    it("disables the toggle when using a remote server", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        const toggle = screen.getByRole("switch");
        expect(toggle).toHaveAttribute("aria-disabled", "true");
      });
    });

    it("refreshes remote state when sharing-status-changed fires", async () => {
      let activeRemoteServer: string | null = null;

      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(activeRemoteServer);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });

      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByRole("switch")).not.toHaveAttribute("aria-disabled", "true");
      });

      activeRemoteServer = "remote-server-1";
      for (const listener of eventListeners.get("sharing-status-changed") ?? []) {
        listener({ payload: null });
      }

      await waitFor(() => {
        expect(screen.getByText("Using remote Voicetypr")).toBeInTheDocument();
      });
    });
  });

  describe("firewall warning", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: true,
              app_allowed: false,
              may_be_blocked: true,
            });
          case "open_firewall_settings":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows firewall warning when may_be_blocked is true", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Firewall may block connections")).toBeInTheDocument();
      });

      expect(screen.getByText(/Your macOS firewall is enabled/)).toBeInTheDocument();
    });

    it("has a link to open system settings", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Open System Settings")).toBeInTheDocument();
      });
    });

    it("can trigger check again for firewall status", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Check again")).toBeInTheDocument();
      });

      await user.click(screen.getByText("Check again"));

      await waitFor(() => {
        expect(toast.info).toHaveBeenCalledWith("Checking firewall status...");
      });
    });
  });

  describe("toggle sharing functionality", () => {
    beforeEach(() => {
      vi.clearAllMocks();
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "start_sharing":
            return Promise.resolve();
          case "stop_sharing":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("calls start_sharing when toggle is turned on", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByRole("switch")).not.toHaveAttribute("aria-disabled", "true");
      });

      await user.click(screen.getByRole("switch"));

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "start_sharing",
          expect.objectContaining({
            port: 47842,
            password: null,
            preservePassword: true,
            serverName: null,
          }),
        );
      });
    });

    it("shows success toast when sharing is enabled", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByRole("switch")).not.toHaveAttribute("aria-disabled", "true");
      });

      await user.click(screen.getByRole("switch"));

      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith("Remote transcription enabled");
      });
    });

    it("shows error toast when start_sharing fails", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "start_sharing") {
          return Promise.reject(new Error("Port already in use"));
        }
        if (command === "get_settings") {
          return Promise.resolve({
            current_model: "large-v3-turbo",
            auto_insert: true,
            launch_at_startup: false,
          });
        }
        if (command === "get_sharing_status") {
          return Promise.resolve({
            enabled: false,
            port: null,
            model_name: null,
            server_name: null,
            active_connections: 0,
          });
        }
        if (command === "get_local_ips") {
          return Promise.resolve(["192.168.1.100 (eth0)"]);
        }
        if (command === "get_model_status") {
          return Promise.resolve({
            models: [shareableModel()],
          });
        }
        if (command === "get_active_remote_server") {
          return Promise.resolve(null);
        }
        if (command === "get_firewall_status") {
          return Promise.resolve({
            firewall_enabled: false,
            app_allowed: true,
            may_be_blocked: false,
          });
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByRole("switch")).not.toHaveAttribute("aria-disabled", "true");
      });

      await user.click(screen.getByRole("switch"));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith("Port already in use");
      });
    });
  });

  describe("copy address functionality", () => {
    beforeEach(() => {
      vi.clearAllMocks();
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows success toast when copy button is clicked", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Ready for remote transcription")).toBeInTheDocument();
      });

      // Find and click the copy button
      const copyButton = screen.getByRole("button", { name: "Copy address 192.168.1.100:47842" });
      fireEvent.click(copyButton);

      // Verify the toast is shown (the clipboard call is a browser API side effect)
      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith("Address copied to clipboard");
      });
    });

    it("has a copy button for each IP address", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Ready for remote transcription")).toBeInTheDocument();
      });

      // Should have at least one copy button
      const copyButtons = screen.getAllByRole("button", { name: /Copy address/ });
      expect(copyButtons.length).toBeGreaterThan(0);
    });
  });

  describe("port configuration", () => {
    beforeEach(() => {
      vi.clearAllMocks();
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            return Promise.resolve();
          case "save_settings":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows port input field when sharing is enabled", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Port")).toBeInTheDocument();
      });
    });

    it("shows save button when port is changed", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Port")).toBeInTheDocument();
      });

      const portInput = screen.getByLabelText("Port");
      await user.clear(portInput);
      await user.type(portInput, "8080");

      await waitFor(() => {
        expect(screen.getByTitle("Save and restart server")).toBeInTheDocument();
      });
    });

    it("advertises the active (saved) port in connect addresses, not an unsaved port edit", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      // Card is live on port 47842.
      await waitFor(() => {
        expect(screen.getByText(/192\.168\.1\.100:47842/)).toBeInTheDocument();
      });

      // Edit the port field without applying it; the server still listens on 47842.
      const portInput = screen.getByLabelText("Port");
      await user.clear(portInput);
      await user.type(portInput, "8080");

      // The advertised connect address must keep showing the listening port...
      expect(screen.getByText(/192\.168\.1\.100:47842/)).toBeInTheDocument();
      expect(screen.queryByText(/192\.168\.1\.100:8080/)).not.toBeInTheDocument();
      // ...and the copy button must advertise the listening port too (matches clipboard).
      expect(
        screen.getByRole("button", { name: "Copy address 192.168.1.100:47842" }),
      ).toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "Copy address 192.168.1.100:8080" }),
      ).not.toBeInTheDocument();
    });
  });

  describe("password configuration", () => {
    beforeEach(() => {
      vi.clearAllMocks();
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
              sharing_password: "",
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
              password: null,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            return Promise.resolve();
          case "save_settings":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows password input field when sharing is enabled", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Password (Optional)")).toBeInTheDocument();
      });
    });

    it("password field starts as hidden (type=password)", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        const passwordInput = screen.getByLabelText("Password (Optional)");
        expect(passwordInput).toHaveAttribute("type", "password");
      });
    });

    it("toggles password visibility when eye icon is clicked", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Password (Optional)")).toBeInTheDocument();
      });

      const passwordInput = screen.getByLabelText("Password (Optional)");
      expect(passwordInput).toHaveAttribute("type", "password");

      // Find and click the visibility toggle (the Eye icon button)
      const toggleButtons = screen.getAllByRole("button");
      const visibilityToggle = toggleButtons.find((btn) => btn.getAttribute("tabindex") === "-1");

      if (visibilityToggle) {
        await user.click(visibilityToggle);

        await waitFor(() => {
          expect(passwordInput).toHaveAttribute("type", "text");
        });
      }
    });

    it("shows save button when password is changed", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Password (Optional)")).toBeInTheDocument();
      });

      const passwordInput = screen.getByLabelText("Password (Optional)");
      await user.type(passwordInput, "secret123");

      await waitFor(() => {
        expect(screen.getByTitle("Save password")).toBeInTheDocument();
      });
    });

    it("persists remote model control opt-in without restarting sharing", async () => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve(sharingStatus());
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({ models: [shareableModel()] });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "update_remote_model_control_enabled":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });

      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      const toggle = await screen.findByRole("switch", {
        name: "Allow trusted devices to change shared model",
      });
      expect(toggle).not.toBeChecked();

      await user.click(toggle);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("update_remote_model_control_enabled", {
          enabled: true,
        });
      });

      expect(
        screen.getByText(
          "Requires a sharing password so only trusted devices can change the model this device shares.",
        ),
      ).toBeInTheDocument();
    });
    it("allows removing a saved sharing password", async () => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
              password_configured: true,
              binding_results: [],
              allow_model_control: false,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });

      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      const removePassword = await screen.findByTitle("Remove saved password");
      await user.click(removePassword);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith(
          "start_sharing",
          expect.objectContaining({
            port: 47842,
            password: null,
            preservePassword: false,
            serverName: null,
          }),
        );
      });
    });

    it("clears the password input after a successful save and surfaces Remove via the configured flag", async () => {
      let sharingStarted = false;
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
              password_configured: sharingStarted,
              binding_results: [],
              allow_model_control: false,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({ models: [shareableModel()] });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            sharingStarted = true;
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });

      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      const passwordInput = await screen.findByLabelText("Password (Optional)");
      // No password configured yet -> no Remove control.
      expect(screen.queryByTitle("Remove saved password")).not.toBeInTheDocument();

      await user.type(passwordInput, "s3cret!");
      await user.click(screen.getByTitle("Save password"));

      // After a successful save the just-typed secret is gone from the input
      // (no longer revealable via the eye toggle) and the Remove control returns.
      await waitFor(() => {
        expect(passwordInput).toHaveValue("");
      });
      expect(passwordInput).toHaveAttribute("placeholder", "Password saved");
      await waitFor(() => {
        expect(screen.getByTitle("Remove saved password")).toBeInTheDocument();
      });
    });
  });

  describe("server configuration section", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)", "10.0.0.5 (WiFi)"]);
          case "get_model_status":
            return Promise.resolve({
              models: [shareableModel()],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("displays multiple IP addresses when available", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText(/192\.168\.1\.100:47842/)).toBeInTheDocument();
        expect(screen.getByText(/10\.0\.0\.5:47842/)).toBeInTheDocument();
      });
    });

    it("shows interface names for each IP", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("(eth0)")).toBeInTheDocument();
        expect(screen.getByText("(WiFi)")).toBeInTheDocument();
      });
    });

    it("shows connect from another device label", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Connect from another device")).toBeInTheDocument();
      });
    });

    it("shows Connection Settings section", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Connection Settings")).toBeInTheDocument();
      });
    });
  });
  describe("when Soniox or cloud is selected", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "soniox",
              current_model_engine: "soniox",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: false,
              port: null,
              model_name: null,
              server_name: null,
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve([]);
          case "get_model_status":
            return Promise.resolve({
              models: [
                shareableModel(),
                {
                  name: "soniox",
                  display_name: "Soniox",
                  downloaded: true,
                  engine: "soniox",
                  kind: "cloud",
                },
              ],
            });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("disables sharing when the current source cannot be shared", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("Current model cannot be shared")).toBeInTheDocument();
      });

      expect(screen.getByRole("switch")).toHaveAttribute("aria-disabled", "true");
      expect(
        screen.getByText(/Cloud sources cannot be shared over the network/),
      ).toBeInTheDocument();
    });
  });

  describe("when sharing is enabled without a network address", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              current_model_engine: "whisper",
              auto_insert: true,
              launch_at_startup: false,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
              binding_results: [],
            });
          case "get_local_ips":
            return Promise.resolve([]);
          case "get_model_status":
            return Promise.resolve({ models: [shareableModel()] });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("shows an empty state instead of a fake network address", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByText("No network address available")).toBeInTheDocument();
      });

      expect(screen.getByText(/Connect this device to Wi-Fi or Ethernet/)).toBeInTheDocument();
      expect(screen.queryByText("Starting server...")).not.toBeInTheDocument();
    });
  });

  describe("port validation", () => {
    beforeEach(() => {
      vi.clearAllMocks();
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              current_model_engine: "whisper",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({ models: [shareableModel()] });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "start_sharing":
            return Promise.resolve();
          case "stop_sharing":
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("rejects invalid ports before calling stop_sharing", async () => {
      const user = userEvent.setup();
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Port")).toBeInTheDocument();
      });

      const portInput = screen.getByLabelText("Port");
      await user.clear(portInput);
      await user.type(portInput, "70000");
      fireEvent.click(screen.getByTitle("Save and restart server"));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith("Enter a valid port between 1 and 65535");
      });
      expect(mockInvoke).not.toHaveBeenCalledWith("stop_sharing");
    });
    it.each([["1e3"], ["47842abc"], ["-1"], ["1.5"]])(
      "rejects non-digit port strings (%s) before calling stop_sharing",
      async (invalidPort) => {
        const user = userEvent.setup();
        renderWithProviders(<NetworkSharingCard />);

        await waitFor(() => {
          expect(screen.getByLabelText("Port")).toBeInTheDocument();
        });

        const portInput = screen.getByLabelText("Port");
        await user.clear(portInput);
        fireEvent.change(portInput, { target: { value: invalidPort } });
        fireEvent.click(screen.getByTitle("Save and restart server"));

        await waitFor(() => {
          expect(toast.error).toHaveBeenCalledWith("Enter a valid port between 1 and 65535");
        });
        expect(mockInvoke).not.toHaveBeenCalledWith("stop_sharing");
      },
    );

    it("restores sharing when a port change restart fails", async () => {
      const user = userEvent.setup();
      let startSharingCalls = 0;
      mockInvoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              current_model_engine: "whisper",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({ models: [shareableModel()] });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            startSharingCalls += 1;
            if (startSharingCalls === 1 && args?.port === 8080) {
              return Promise.reject(new Error("port in use"));
            }
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });

      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Port")).toBeInTheDocument();
      });

      const portInput = screen.getByLabelText("Port");
      await user.clear(portInput);
      fireEvent.change(portInput, { target: { value: "8080" } });
      fireEvent.click(screen.getByTitle("Save and restart server"));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith("Failed to update port; sharing restored");
      });
      expect(mockInvoke).toHaveBeenCalledWith(
        "start_sharing",
        expect.objectContaining({ port: 47842 }),
      );
    });

    it("restores sharing when a password change restart fails", async () => {
      const user = userEvent.setup();
      let startSharingCalls = 0;
      mockInvoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              current_model_engine: "whisper",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
              password_configured: true,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({ models: [shareableModel()] });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          case "stop_sharing":
            return Promise.resolve();
          case "start_sharing":
            startSharingCalls += 1;
            if (startSharingCalls === 1 && args?.password === "newpass") {
              return Promise.reject(new Error("auth failed"));
            }
            return Promise.resolve();
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });

      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(screen.getByLabelText("Password (Optional)")).toBeInTheDocument();
      });

      const passwordInput = screen.getByLabelText("Password (Optional)");
      await user.type(passwordInput, "newpass");
      fireEvent.click(screen.getByTitle("Save password"));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith("Failed to update password; sharing restored");
      });
      expect(mockInvoke).toHaveBeenCalledWith(
        "start_sharing",
        expect.objectContaining({ port: 47842 }),
      );
    });
  });

  describe("host password copy", () => {
    beforeEach(() => {
      mockInvoke.mockImplementation((command: string) => {
        switch (command) {
          case "get_settings":
            return Promise.resolve({
              current_model: "large-v3-turbo",
              current_model_engine: "whisper",
              auto_insert: true,
              launch_at_startup: false,
              sharing_port: 47842,
            });
          case "get_sharing_status":
            return Promise.resolve({
              enabled: true,
              port: 47842,
              model_name: "large-v3-turbo",
              server_name: "My-PC",
              active_connections: 0,
              password_configured: false,
            });
          case "get_local_ips":
            return Promise.resolve(["192.168.1.100 (eth0)"]);
          case "get_model_status":
            return Promise.resolve({ models: [shareableModel()] });
          case "get_active_remote_server":
            return Promise.resolve(null);
          case "get_firewall_status":
            return Promise.resolve({
              firewall_enabled: false,
              app_allowed: true,
              may_be_blocked: false,
            });
          default:
            return Promise.reject(new Error(`Unknown command: ${command}`));
        }
      });
    });

    it("explains that connecting devices need the host password", async () => {
      renderWithProviders(<NetworkSharingCard />);

      await waitFor(() => {
        expect(
          screen.getByText(
            "Other devices need this password to connect to your shared transcription.",
          ),
        ).toBeInTheDocument();
      });
    });
  });
});
