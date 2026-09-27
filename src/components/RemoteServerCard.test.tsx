import { render, screen, waitFor, fireEvent, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock Tauri invoke
const mockInvoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const mockToastSuccess = vi.fn();
const mockToastError = vi.fn();
vi.mock("sonner", () => ({
  toast: {
    success: (...args: unknown[]) => mockToastSuccess(...args),
    error: (...args: unknown[]) => mockToastError(...args),
  },
}));

import { RemoteServerCard, SavedConnection, StatusResponse } from "./RemoteServerCard";

// Test data factory
function createMockServer(overrides: Partial<SavedConnection> = {}): SavedConnection {
  return {
    id: "server-1",
    host: "192.168.1.100",
    port: 47842,
    password: null,
    name: null,
    created_at: Date.now(),
    model: "large-v3-turbo",
    status: "Online",
    ...overrides,
  };
}

function createMockRemoteControl(overrides: Record<string, unknown> = {}) {
  return {
    current: {
      id: "large-v3-turbo",
      display_name: "Large V3 Turbo",
      engine: "whisper",
    },
    available: [
      {
        id: "large-v3-turbo",
        display_name: "Large V3 Turbo",
        engine: "whisper",
      },
      {
        id: "base.en",
        display_name: "Base English",
        engine: "whisper",
      },
    ],
    ...overrides,
  };
}

function createMockStatusResponse(overrides: Partial<StatusResponse> = {}): StatusResponse {
  return {
    status: "ok",
    version: "1.0.0",
    model: "large-v3-turbo",
    name: "Test Server",
    machine_id: "remote-machine-123",
    ...overrides,
  };
}

describe("RemoteServerCard", () => {
  const mockOnSelect = vi.fn();
  const mockOnRemove = vi.fn();
  const mockOnEdit = vi.fn();
  const mockOnDeselect = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();

    // Default: server is online
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_local_machine_id") {
        return Promise.resolve("local-machine-456");
      }
      if (command === "test_remote_server") {
        return Promise.resolve(createMockStatusResponse());
      }
      if (command === "get_remote_transcription_control") {
        return Promise.resolve(createMockRemoteControl());
      }
      return Promise.reject(new Error(`Unknown command: ${command}`));
    });
  });

  // ============================================================================
  // Basic Rendering Tests
  // ============================================================================

  describe("basic rendering", () => {
    it("renders server display name when no custom name is set", async () => {
      const server = createMockServer({ name: null });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      // The host:port should be shown as the display name
      expect(screen.getByText("192.168.1.100:47842")).toBeInTheDocument();
    });

    it("renders custom server name when set", async () => {
      const server = createMockServer({ name: "My Home Server" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      expect(screen.getByText("My Home Server")).toBeInTheDocument();
    });

    it("renders accessible Edit and Remove buttons", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      expect(screen.getByRole("button", { name: "Edit 192.168.1.100:47842" })).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Remove 192.168.1.100:47842" }),
      ).toBeInTheDocument();
    });
  });

  // ============================================================================
  // Status Display Tests
  // ============================================================================

  describe("status display", () => {
    it("shows Online status when server responds successfully", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });
    });

    it("shows model name when server is online", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        // Model is displayed with a bullet prefix and user-facing display name.
        expect(screen.getByText(/Large v3 Turbo/)).toBeInTheDocument();
      });
    });

    it("shows Offline status when server fails to respond", async () => {
      const server = createMockServer({ status: "Offline" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Offline")).toBeInTheDocument();
      });
    });

    it("shows Password incorrect status when authentication fails", async () => {
      const server = createMockServer({ status: "AuthFailed" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Password incorrect")).toBeInTheDocument();
      });
    });

    it("shows This Machine status for self-connection", async () => {
      const server = createMockServer({ status: "SelfConnection" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("This Machine")).toBeInTheDocument();
      });

      // Self-connections explain that the saved connection points back to this device.
      expect(screen.getByText(/This is the same device/)).toBeInTheDocument();
    });
  });

  // ============================================================================
  // Active State Tests
  // ============================================================================

  describe("active state", () => {
    it("shows Routing to badge when isActive is true and server is online", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={true}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText(/Routing to/)).toBeInTheDocument();
      });
    });

    it("shows warning badge instead of positive Routing to copy when active server is offline", async () => {
      const server = createMockServer({ status: "Offline" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={true}
            onSelect={mockOnSelect}
            onDeselect={mockOnDeselect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      expect(screen.getByText(/Routing risk: Offline/)).toBeInTheDocument();
      expect(screen.queryByText(/Routing to/)).not.toBeInTheDocument();
    });

    it("does not show Routing to badge when isActive is false", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });

      expect(screen.queryByText(/Routing to/)).not.toBeInTheDocument();
    });
  });

  // ============================================================================
  // Interaction Tests
  // ============================================================================

  describe("interactions", () => {
    it("calls onSelect when clicking card while server is online", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });

      const card = screen.getByText("192.168.1.100:47842").closest("[class*='p-4']");
      await act(async () => {
        fireEvent.click(card!);
      });

      expect(mockOnSelect).toHaveBeenCalledWith("server-1");
    });

    it("still calls onSelect when clicking card while server is offline", async () => {
      const server = createMockServer({ status: "Offline" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Offline")).toBeInTheDocument();
      });

      const card = screen.getByText("192.168.1.100:47842").closest("[class*='p-4']");
      await act(async () => {
        fireEvent.click(card!);
      });

      expect(mockOnSelect).toHaveBeenCalledWith("server-1");
    });

    it("does not deselect when clicking an already-active card", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={true}
            onSelect={mockOnSelect}
            onDeselect={mockOnDeselect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      const card = screen.getByText("192.168.1.100:47842").closest("[class*='p-4']");
      await act(async () => {
        fireEvent.click(card!);
      });

      expect(mockOnSelect).not.toHaveBeenCalled();
      expect(mockOnDeselect).not.toHaveBeenCalled();
    });

    it("calls onDeselect only from the explicit Stop routing button", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={true}
            onSelect={mockOnSelect}
            onDeselect={mockOnDeselect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: "Stop routing" }));
      });

      expect(mockOnDeselect).toHaveBeenCalledTimes(1);
      expect(mockOnSelect).not.toHaveBeenCalled();
    });

    it("calls onEdit when clicking Edit button", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      const editButton = screen.getByTitle("Edit server");
      await act(async () => {
        fireEvent.click(editButton);
      });

      expect(mockOnEdit).toHaveBeenCalledWith(server);
    });

    it("calls onRemove when clicking Remove button", async () => {
      mockOnRemove.mockResolvedValue(undefined);

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      const removeButton = screen.getByTitle("Remove server");
      await act(async () => {
        fireEvent.click(removeButton);
      });

      expect(mockOnRemove).toHaveBeenCalledWith("server-1");
    });

    it("Edit button click does not propagate to card selection", async () => {
      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });

      const editButton = screen.getByTitle("Edit server");
      await act(async () => {
        fireEvent.click(editButton);
      });

      expect(mockOnEdit).toHaveBeenCalled();
      expect(mockOnSelect).not.toHaveBeenCalled();
    });

    it("Remove button click does not propagate to card selection", async () => {
      mockOnRemove.mockResolvedValue(undefined);

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });

      const removeButton = screen.getByTitle("Remove server");
      await act(async () => {
        fireEvent.click(removeButton);
      });

      expect(mockOnRemove).toHaveBeenCalled();
      expect(mockOnSelect).not.toHaveBeenCalled();
    });
  });

  // ============================================================================
  // Self-Connection Tests
  // ============================================================================

  describe("self-connection handling", () => {
    it("does not allow selection when server is self", async () => {
      const server = createMockServer({ status: "SelfConnection" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("This Machine")).toBeInTheDocument();
      });

      const card = screen.getByText("192.168.1.100:47842").closest("[class*='p-4']");
      await act(async () => {
        fireEvent.click(card!);
      });

      expect(mockOnSelect).not.toHaveBeenCalled();
    });
  });

  // ============================================================================
  // Edge Cases
  // ============================================================================

  describe("edge cases", () => {
    it("handles different port numbers correctly", async () => {
      const server = createMockServer({ host: "10.0.0.5", port: 8080 });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      expect(screen.getByText("10.0.0.5:8080")).toBeInTheDocument();
    });

    it("handles server with password", async () => {
      const server = createMockServer({ password: "secret123" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
      });
    });
  });
  // ============================================================================
  // Remote transcription model control
  // ============================================================================

  describe("remote transcription model control", () => {
    const mockOnServerUpdated = vi.fn();

    beforeEach(() => {
      mockOnServerUpdated.mockReset();
      mockToastSuccess.mockReset();
      mockToastError.mockReset();
      Element.prototype.scrollIntoView = vi.fn();
    });

    it("loads and shows host shared transcription model control for online servers", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_remote_transcription_control") {
          return Promise.resolve(createMockRemoteControl());
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={true}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
            onServerUpdated={mockOnServerUpdated}
          />,
        );
      });

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("get_remote_transcription_control", {
          serverId: server.id,
        });
      });

      expect(screen.getByText("Transcription model on 192.168.1.100:47842")).toBeInTheDocument();
      expect(
        screen.getByText(
          "Changes the host's shared transcription model, not this device's local model.",
        ),
      ).toBeInTheDocument();
      expect(
        screen.getByRole("combobox", { name: "Transcription model on 192.168.1.100:47842" }),
      ).toBeInTheDocument();
    });

    it("shows locked message when remote control requires a password", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_remote_transcription_control") {
          return Promise.reject(
            new Error("Remote model control requires a sharing password on the server."),
          );
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(
          screen.getByText(
            "Add a sharing password on the host so only trusted devices can change the shared model.",
          ),
        ).toBeInTheDocument();
      });
    });

    it("shows quiet unavailable copy when host disabled remote model control", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_remote_transcription_control") {
          return Promise.reject(new Error("Remote model control is disabled on this device."));
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(
          screen.getByText("The host has not enabled remote model changes."),
        ).toBeInTheDocument();
      });
    });

    it("shows unsupported locked message without breaking the card", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_remote_transcription_control") {
          return Promise.reject(new Error("Server error: 404 Not Found"));
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      await waitFor(() => {
        expect(screen.getByText("Online")).toBeInTheDocument();
        expect(
          screen.getByText("Remote model changes are not available for this connection."),
        ).toBeInTheDocument();
      });
    });

    it("shows offline locked message without fetching remote control", async () => {
      const server = createMockServer({ status: "Offline" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      expect(
        screen.getByText("Remote model control is unavailable while the host is offline."),
      ).toBeInTheDocument();
      expect(mockInvoke).not.toHaveBeenCalledWith(
        "get_remote_transcription_control",
        expect.anything(),
      );
    });

    it("updates remote shared model and refreshes server state", async () => {
      mockInvoke.mockImplementation((command: string) => {
        if (command === "get_remote_transcription_control") {
          return Promise.resolve(createMockRemoteControl());
        }
        if (command === "update_remote_transcription_control") {
          return Promise.resolve(
            createMockRemoteControl({
              current: {
                id: "base.en",
                display_name: "Base English",
                engine: "whisper",
              },
            }),
          );
        }
        return Promise.reject(new Error(`Unknown command: ${command}`));
      });

      const server = createMockServer();

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={true}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
            onServerUpdated={mockOnServerUpdated}
          />,
        );
      });

      await waitFor(() => {
        expect(
          screen.getByRole("combobox", { name: "Transcription model on 192.168.1.100:47842" }),
        ).toBeInTheDocument();
      });

      const select = screen.getByRole("combobox", {
        name: "Transcription model on 192.168.1.100:47842",
      });

      const user = userEvent.setup();
      await user.click(select);

      const nextModel = await screen.findByText("Base English");
      await user.click(nextModel);

      await waitFor(() => {
        expect(mockInvoke).toHaveBeenCalledWith("update_remote_transcription_control", {
          serverId: server.id,
          currentModel: "base.en",
          currentModelEngine: "whisper",
        });
      });

      expect(mockToastSuccess).toHaveBeenCalled();
      expect(mockOnServerUpdated).toHaveBeenCalled();
    });

    it("shows edit password action when authentication fails", async () => {
      const server = createMockServer({ status: "AuthFailed" });

      await act(async () => {
        render(
          <RemoteServerCard
            server={server}
            isActive={false}
            onSelect={mockOnSelect}
            onRemove={mockOnRemove}
            onEdit={mockOnEdit}
          />,
        );
      });

      expect(screen.getAllByText("Edit password").length).toBeGreaterThan(0);
      expect(
        screen.getByText("Password incorrect. Tap edit to update the saved password."),
      ).toBeInTheDocument();

      fireEvent.click(screen.getAllByText("Edit password")[0]);
      expect(mockOnEdit).toHaveBeenCalledWith(server);
    });
  });
});
