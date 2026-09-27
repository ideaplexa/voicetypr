import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AgentCliSection } from "../AgentCliSection";

const mockInvoke = vi.fn();
const writeTextMock = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
  },
}));

describe("AgentCliSection", () => {
  const healthyStatus = {
    installed: true,
    manageable: true,
    path: "/usr/local/bin/voicetypr",
    app_version: "2.0.5",
    command_version: "2.0.5",
    compatible: true,
    detail: null,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mockInvoke.mockReset();
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: writeTextMock },
    });
    writeTextMock.mockClear();
  });

  it("installs a missing managed command", async () => {
    const missingStatus = {
      ...healthyStatus,
      installed: false,
      path: null,
      command_version: null,
      compatible: false,
    };
    mockInvoke.mockResolvedValueOnce(missingStatus).mockResolvedValueOnce(healthyStatus);

    render(<AgentCliSection />);

    const installButton = await screen.findByRole("button", { name: /^install$/i });
    fireEvent.click(installButton);

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("install_cli_tool");
    });
    expect(await screen.findByText("Ready and compatible")).toBeInTheDocument();
    expect(screen.getByText("CLI v2.0.5")).toBeInTheDocument();
  });

  it("repairs an installed command that targets another app version", async () => {
    mockInvoke
      .mockResolvedValueOnce({
        ...healthyStatus,
        command_version: null,
        compatible: false,
        detail: "The command points to a different Voicetypr installation.",
      })
      .mockResolvedValueOnce(healthyStatus);

    render(<AgentCliSection />);

    fireEvent.click(await screen.findByRole("button", { name: /^update$/i }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("repair_cli_tool");
    });
    expect(await screen.findByText("Ready and compatible")).toBeInTheDocument();
  });

  it("offers repair and removal for a healthy installed command", async () => {
    mockInvoke.mockResolvedValueOnce(healthyStatus).mockResolvedValueOnce({
      ...healthyStatus,
      installed: false,
      path: null,
      command_version: null,
      compatible: false,
    });

    render(<AgentCliSection />);

    expect(await screen.findByRole("button", { name: /^repair$/i })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /^remove$/i }));

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("uninstall_cli_tool");
    });
    expect(await screen.findByText("Not installed")).toBeInTheDocument();
  });

  it("shows an unmanaged conflict without destructive actions", async () => {
    mockInvoke.mockResolvedValue({
      ...healthyStatus,
      manageable: false,
      command_version: null,
      compatible: false,
      detail: "Another command already uses this path. Voicetypr will not overwrite it.",
    });

    render(<AgentCliSection />);

    expect(await screen.findByText(/another command already uses this path/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^install$/i })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^remove$/i })).not.toBeInTheDocument();
    expect(screen.getAllByText(/voicetypr transcribe/).length).toBeGreaterThan(0);
  });

  it("copies a reusable prompt for terminal-capable agents", async () => {
    mockInvoke.mockResolvedValue(healthyStatus);
    render(<AgentCliSection />);

    fireEvent.click(await screen.findByRole("button", { name: /copy agent prompt/i }));

    expect(writeTextMock).toHaveBeenCalledWith(
      expect.stringContaining("voicetypr transcribe --file <file> --json"),
    );
    expect(screen.getByText(/Claude Code, Codex, OpenCode/)).toBeInTheDocument();
  });
});
