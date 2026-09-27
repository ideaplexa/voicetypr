import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TrayStatus } from "@/lib/tray";
import { AppShell } from "./AppShell";

const getTrayStatusMock = vi.fn<() => Promise<TrayStatus>>();
const retryTrayCreationMock = vi.fn<() => Promise<TrayStatus>>();
let trayStatusListener: ((event: { payload: TrayStatus }) => void) | undefined;

vi.mock("@/lib/tray", () => ({
  getTrayStatus: () => getTrayStatusMock(),
  retryTrayCreation: () => retryTrayCreationMock(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_eventName: string, listener: (event: { payload: TrayStatus }) => void) => {
    trayStatusListener = listener;
    return vi.fn();
  }),
}));

vi.mock("@/components/Sidebar", () => ({
  Sidebar: () => <aside>Sidebar</aside>,
}));

vi.mock("@/components/tabs/TabContainer", () => ({
  TabContainer: () => <main>Active section</main>,
}));

vi.mock("@/components/ui/sidebar", () => ({
  SidebarProvider: ({ children }: { children: ReactNode }) => <>{children}</>,
  SidebarInset: ({ children, className }: { children: ReactNode; className?: string }) => (
    <section className={className}>{children}</section>
  ),
  SidebarTrigger: ({ className }: { className?: string }) => (
    <button type="button" className={className}>
      Toggle Sidebar
    </button>
  ),
}));

describe("AppShell tray recovery", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    trayStatusListener = undefined;
  });

  it("keeps recovery help visible until a manual retry restores the tray", async () => {
    const user = userEvent.setup();
    getTrayStatusMock.mockResolvedValue({
      available: false,
      attempts: 8,
      lastError: "status area unavailable",
    });
    retryTrayCreationMock.mockResolvedValue({
      available: true,
      attempts: 9,
      lastError: null,
    });

    render(<AppShell activeSection="overview" onSectionChange={vi.fn()} />);

    expect(await screen.findByText("Menu-bar icon unavailable")).toBeInTheDocument();
    expect(screen.getByText(/after 8 attempts/i)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Retry icon" }));

    expect(retryTrayCreationMock).toHaveBeenCalledOnce();
    await waitFor(() => {
      expect(screen.queryByText("Menu-bar icon unavailable")).not.toBeInTheDocument();
    });
  });

  it("reacts when deferred backend recovery changes tray availability", async () => {
    getTrayStatusMock.mockResolvedValue({
      available: false,
      attempts: 5,
      lastError: "startup failure",
    });

    render(<AppShell activeSection="overview" onSectionChange={vi.fn()} />);

    expect(await screen.findByText("Menu-bar icon unavailable")).toBeInTheDocument();
    expect(trayStatusListener).toBeDefined();

    act(() => {
      trayStatusListener?.({
        payload: { available: true, attempts: 6, lastError: null },
      });
    });

    expect(screen.queryByText("Menu-bar icon unavailable")).not.toBeInTheDocument();
  });

  it("aligns the sidebar toggle with the native window controls", async () => {
    getTrayStatusMock.mockResolvedValue({
      available: true,
      attempts: 0,
      lastError: null,
    });

    render(<AppShell activeSection="overview" onSectionChange={vi.fn()} />);

    const titleBar = screen.getByRole("banner");
    const toggle = screen.getByRole("button", { name: "Toggle Sidebar" });
    const mainSurface = screen.getByText("Active section").closest("section");
    expect(titleBar).toHaveAttribute("data-tauri-drag-region");
    expect(titleBar).toHaveClass("h-9");
    expect(toggle).toHaveClass("translate-y-1");
    expect(mainSurface).toHaveClass("rounded-2xl", "bg-background");
    expect(mainSurface).not.toHaveClass("border");
    expect(mainSurface).not.toHaveClass("shadow-sm");
  });
});
