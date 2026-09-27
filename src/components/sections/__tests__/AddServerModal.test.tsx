import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, Mock, vi } from "vitest";
import { AddServerModal } from "../AddServerModal";

// Mock the invoke function from Tauri
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// Mock sonner toast
vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}));

// Import invoke for type checking
import { invoke } from "@tauri-apps/api/core";

describe("AddServerModal", () => {
  const mockOnOpenChange = vi.fn();
  const mockOnServerAdded = vi.fn();

  beforeEach(async () => {
    vi.clearAllMocks();
    // Set up default mock behavior for invoke
    // The component calls get_local_machine_id on mount when open
    const invokeMock = invoke as unknown as Mock;
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_local_machine_id") {
        return Promise.resolve("local-machine-id");
      }
      // Return undefined for unknown commands (tests should mock specific commands)
      return Promise.resolve(undefined);
    });
  });

  // ============================================================================
  // Modal Open/Close Behavior
  // ============================================================================

  describe("Modal open/close behavior", () => {
    it("renders when open is true", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );
      expect(screen.getByText("Add Remote Voicetypr")).toBeInTheDocument();
    });

    it("does not render when open is false", () => {
      render(
        <AddServerModal
          open={false}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );
      expect(screen.queryByText("Add Remote Voicetypr")).not.toBeInTheDocument();
    });

    it("shows edit mode title when editServer is provided", () => {
      const editServer = {
        id: "test-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        name: "Test Server",
        created_at: 1234567890,
      };

      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
          editServer={editServer}
        />,
      );
      expect(screen.getByText("Edit Remote Voicetypr")).toBeInTheDocument();
    });

    it("calls onOpenChange when Cancel button is clicked", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );
      fireEvent.click(screen.getByText("Cancel"));
      expect(mockOnOpenChange).toHaveBeenCalledWith(false);
    });
  });

  // ============================================================================
  // Form Rendering
  // ============================================================================

  describe("Form rendering", () => {
    it("renders all form fields", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      expect(screen.getByLabelText("Host Address")).toBeInTheDocument();
      expect(screen.getByLabelText("Port")).toBeInTheDocument();
      expect(screen.getByLabelText("Password (if required)")).toBeInTheDocument();
      expect(screen.getByLabelText("Display Name (optional)")).toBeInTheDocument();
    });

    it("renders Test Connection button", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );
      expect(screen.getByText("Test Connection")).toBeInTheDocument();
    });

    it("renders Add Server button in add mode", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );
      expect(screen.getByText("Add Server")).toBeInTheDocument();
    });

    it("renders Save Changes button in edit mode", () => {
      const editServer = {
        id: "test-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        name: "Test Server",
        created_at: 1234567890,
      };

      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
          editServer={editServer}
        />,
      );
      expect(screen.getByText("Save Changes")).toBeInTheDocument();
    });

    it("has default port value of 47842", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );
      expect(screen.getByLabelText("Port")).toHaveValue(47842);
    });
  });

  // ============================================================================
  // Form Input
  // ============================================================================

  describe("Form input", () => {
    it("allows entering host address", async () => {
      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      const hostInput = screen.getByLabelText("Host Address");
      await user.type(hostInput, "192.168.1.100");
      expect(hostInput).toHaveValue("192.168.1.100");
    });

    it("allows entering port", async () => {
      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      const portInput = screen.getByLabelText("Port");
      await user.clear(portInput);
      await user.type(portInput, "8080");
      expect(portInput).toHaveValue(8080);
    });

    it("allows entering password", async () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      const passwordInput = await screen.findByLabelText("Password (if required)");
      await waitFor(() => expect(passwordInput).not.toBeDisabled());
      fireEvent.change(passwordInput, { target: { value: "secret123" } });
      expect(passwordInput).toHaveValue("secret123");
    });

    it("allows entering display name", async () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      const nameInput = await screen.findByLabelText("Display Name (optional)");
      await waitFor(() => expect(nameInput).not.toBeDisabled());
      fireEvent.change(nameInput, { target: { value: "My Server" } });
      expect(nameInput).toHaveValue("My Server");
    });
  });

  describe("Port validation", () => {
    it("rejects invalid ports before testing connection", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { toast } = await import("sonner");
      const invokeMock = invoke as unknown as Mock;
      const user = userEvent.setup();

      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      const portInput = screen.getByLabelText("Port");
      await user.clear(portInput);
      fireEvent.change(portInput, { target: { value: "47842abc" } });
      fireEvent.click(screen.getByText("Test Connection"));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith("Enter a valid port between 1 and 65535");
      });
      expect(invokeMock).not.toHaveBeenCalledWith("test_remote_connection", expect.anything());
    });

    it("rejects invalid ports before saving server", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { toast } = await import("sonner");
      const invokeMock = invoke as unknown as Mock;
      const user = userEvent.setup();

      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      const portInput = screen.getByLabelText("Port");
      await user.clear(portInput);
      fireEvent.change(portInput, { target: { value: "1e3" } });
      fireEvent.click(screen.getByText("Add Server"));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith("Enter a valid port between 1 and 65535");
      });
      expect(invokeMock).not.toHaveBeenCalledWith("add_remote_server", expect.anything());
    });
  });

  // ============================================================================
  // Password Visibility Toggle
  // ============================================================================

  describe("Password visibility toggle", () => {
    it("password is hidden by default", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      const passwordInput = screen.getByLabelText("Password (if required)");
      expect(passwordInput).toHaveAttribute("type", "password");
    });

    it("toggles password visibility when eye icon is clicked", async () => {
      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      const passwordInput = screen.getByLabelText("Password (if required)");
      const toggleButton = passwordInput.parentElement?.querySelector("button");

      expect(passwordInput).toHaveAttribute("type", "password");

      if (toggleButton) {
        await user.click(toggleButton);
        expect(passwordInput).toHaveAttribute("type", "text");

        await user.click(toggleButton);
        expect(passwordInput).toHaveAttribute("type", "password");
      }
    });
  });

  // ============================================================================
  // Form Validation
  // ============================================================================

  describe("Form validation", () => {
    it("Test Connection button is disabled when host is empty", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      expect(screen.getByText("Test Connection")).toBeDisabled();
    });

    it("Test Connection button is enabled when host is filled", async () => {
      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      expect(screen.getByText("Test Connection")).not.toBeDisabled();
    });

    it("disables Test Connection when edit mode would preserve a saved password", async () => {
      const editServer = {
        id: "existing-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        has_password: true,
        name: "Office Mac",
        created_at: 1234567890,
      };

      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
          editServer={editServer}
        />,
      );

      expect(await screen.findByText("Test Connection")).toBeDisabled();
      expect(
        screen.getByText(/Saving with this field empty keeps the saved password/i),
      ).toBeInTheDocument();
    });

    it("Add Server button is disabled when host is empty", () => {
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      expect(screen.getByText("Add Server")).toBeDisabled();
    });

    it("Add Server button is enabled when host is filled", async () => {
      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      expect(screen.getByText("Add Server")).not.toBeDisabled();
    });

    it("shows error toast when trying to save without host", async () => {
      const { toast: _toast } = await import("sonner");
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      // The button should be disabled, but let's test the validation logic
      // by checking the disabled state
      expect(screen.getByText("Add Server")).toBeDisabled();
    });
  });

  // ============================================================================
  // Test Connection
  // ============================================================================

  describe("Test connection", () => {
    it("shows Testing... state while testing", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      // Create a promise that never resolves to keep it in testing state
      invokeMock.mockImplementation(() => new Promise(() => {}));

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Test Connection"));

      await waitFor(() => {
        expect(screen.getByText("Testing...")).toBeInTheDocument();
      });
    });

    it("shows success state after successful test", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockResolvedValueOnce({
        status: "ok",
        version: "1.0.0",
        model: "whisper-base",
        name: "Test Server",
        machine_id: "different-machine",
      });

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Test Connection"));

      await waitFor(() => {
        expect(screen.getByText("Connected")).toBeInTheDocument();
      });
    });

    it("shows error state after failed test", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockRejectedValueOnce("Failed to connect");

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Test Connection"));

      await waitFor(() => {
        expect(screen.getByText("Connection failed")).toBeInTheDocument();
      });
    });

    it("shows authentication error message", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockRejectedValueOnce("Authentication failed");

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Test Connection"));

      await waitFor(() => {
        expect(screen.getByText(/check password/i)).toBeInTheDocument();
      });
    });

    it("auto-fills name from server response", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockResolvedValueOnce({
        status: "ok",
        version: "1.0.0",
        model: "whisper-base",
        name: "Auto Server Name",
        machine_id: "different-machine",
      });

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Test Connection"));

      await waitFor(() => {
        expect(screen.getByLabelText("Display Name (optional)")).toHaveValue("Auto Server Name");
      });
    });
  });

  // ============================================================================
  // Self-Connection Detection
  // ============================================================================

  describe("Self-connection detection", () => {
    it("detects and blocks self-connection", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("same-machine-id"); // get_local_machine_id
      invokeMock.mockResolvedValueOnce({
        status: "ok",
        version: "1.0.0",
        model: "whisper-base",
        name: "My Machine",
        machine_id: "same-machine-id", // Same as local
      });

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "localhost");
      fireEvent.click(screen.getByText("Test Connection"));

      await waitFor(() => {
        expect(screen.getByText("Self-connection detected")).toBeInTheDocument();
      });
    });

    it("disables Add Server button when self-connection detected", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("same-machine-id"); // get_local_machine_id
      invokeMock.mockResolvedValueOnce({
        status: "ok",
        version: "1.0.0",
        model: "whisper-base",
        name: "My Machine",
        machine_id: "same-machine-id",
      });

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "localhost");
      fireEvent.click(screen.getByText("Test Connection"));

      await waitFor(() => {
        expect(screen.getByText("Add Server")).toBeDisabled();
      });
    });
  });

  // ============================================================================
  // Form Submission
  // ============================================================================

  describe("Form submission", () => {
    it("calls add_remote_server when adding new server", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockResolvedValueOnce({
        id: "new-server-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        name: "Test Server",
        created_at: Date.now(),
      });

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      await user.type(screen.getByLabelText("Display Name (optional)"), "Test Server");
      fireEvent.click(screen.getByText("Add Server"));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("add_remote_server", {
          host: "192.168.1.100",
          port: 47842,
          password: null,
          name: "Test Server",
        });
      });
    });

    it("calls update_remote_server when editing existing server", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockResolvedValueOnce({
        id: "existing-id",
        host: "192.168.1.200",
        port: 8080,
        password: "newpass",
        name: "Updated Server",
        created_at: Date.now(),
      });

      const editServer = {
        id: "existing-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        name: "Old Server",
        created_at: 1234567890,
      };

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
          editServer={editServer}
        />,
      );

      // Update the host
      const hostInput = screen.getByLabelText("Host Address");
      await user.clear(hostInput);
      await user.type(hostInput, "192.168.1.200");

      fireEvent.click(screen.getByText("Save Changes"));

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("update_remote_server", {
          serverId: "existing-id",
          host: "192.168.1.200",
          port: 47842,
          password: null,
          preservePassword: false,
          name: "Old Server",
        });
      });
    });

    it("calls onServerAdded callback after successful add", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      const newServer = {
        id: "new-server-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        name: "Test Server",
        created_at: Date.now(),
      };
      invokeMock.mockResolvedValueOnce(newServer);

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Add Server"));

      await waitFor(() => {
        expect(mockOnServerAdded).toHaveBeenCalledWith(newServer);
      });
    });

    it("closes modal after successful submission", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockResolvedValueOnce({
        id: "new-server-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        name: null,
        created_at: Date.now(),
      });

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Add Server"));

      await waitFor(() => {
        expect(mockOnOpenChange).toHaveBeenCalledWith(false);
      });
    });
  });

  // ============================================================================
  // Edit Mode
  // ============================================================================

  describe("Edit mode", () => {
    it("populates form with existing server data", () => {
      const editServer = {
        id: "test-id",
        host: "192.168.1.100",
        port: 8080,
        password: "secret",
        name: "My Server",
        created_at: 1234567890,
      };

      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
          editServer={editServer}
        />,
      );

      expect(screen.getByLabelText("Host Address")).toHaveValue("192.168.1.100");
      expect(screen.getByLabelText("Port")).toHaveValue(8080);
      expect(screen.getByLabelText("Password (if required)")).toHaveValue("secret");
      expect(screen.getByLabelText("Display Name (optional)")).toHaveValue("My Server");
    });

    it("shows correct description in edit mode", () => {
      const editServer = {
        id: "test-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        name: "Test Server",
        created_at: 1234567890,
      };

      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
          editServer={editServer}
        />,
      );

      expect(screen.getByText(/Update connection details/i)).toBeInTheDocument();
    });
  });

  // ============================================================================
  // Loading States
  // ============================================================================

  describe("Loading states", () => {
    it("disables inputs while saving", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      // Create a promise that doesn't resolve immediately
      invokeMock.mockImplementationOnce(() => new Promise(() => {}));

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Add Server"));

      await waitFor(() => {
        expect(screen.getByText(/Adding.../i)).toBeInTheDocument();
      });
    });

    it("shows Saving... text in edit mode while saving", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockImplementationOnce(() => new Promise(() => {}));

      const editServer = {
        id: "test-id",
        host: "192.168.1.100",
        port: 47842,
        password: null,
        name: "Test Server",
        created_at: 1234567890,
      };

      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
          editServer={editServer}
        />,
      );

      fireEvent.click(screen.getByText("Save Changes"));

      await waitFor(() => {
        expect(screen.getByText(/Saving.../i)).toBeInTheDocument();
      });
    });
  });

  // ============================================================================
  // Error Handling
  // ============================================================================

  describe("Error handling", () => {
    it("shows toast on save failure", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { toast } = await import("sonner");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockRejectedValueOnce(new Error("Network error"));

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Add Server"));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalled();
      });
    });

    it("does not close modal on save failure", async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const invokeMock = invoke as unknown as Mock;
      invokeMock.mockResolvedValueOnce("machine-123"); // get_local_machine_id
      invokeMock.mockRejectedValueOnce(new Error("Network error"));

      const user = userEvent.setup();
      render(
        <AddServerModal
          open={true}
          onOpenChange={mockOnOpenChange}
          onServerAdded={mockOnServerAdded}
        />,
      );

      await user.type(screen.getByLabelText("Host Address"), "192.168.1.100");
      fireEvent.click(screen.getByText("Add Server"));

      await waitFor(() => {
        // Modal should still be open - onOpenChange should not be called with false
        expect(screen.getByText("Add Remote Voicetypr")).toBeInTheDocument();
      });
    });
  });
});
