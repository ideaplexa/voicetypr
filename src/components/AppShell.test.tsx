import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TrayStatus } from "@/lib/tray";
import { AppShell } from "./AppShell";

const platform = vi.hoisted(() => ({ current: "macos" }));
vi.mock("@/lib/platform", () => ({
  get isMacOS() {
    return platform.current === "macos";
  },
}));

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
  Sidebar: ({ onSectionChange }: { onSectionChange: (section: string) => void }) => (
    <aside>
      <button onClick={() => onSectionChange("history")}>History</button>
    </aside>
  ),
}));

vi.mock("@/components/tabs/TabContainer", () => ({
  TabContainer: ({ activeSection }: { activeSection: string }) => <div>{activeSection}</div>,
}));

describe("AppShell tray recovery", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    platform.current = "macos";
    trayStatusListener = undefined;
    document.cookie = "sidebar_state=true; path=/";
  });

  it.each(["macos", "windows"])(
    "restores the saved rail and keeps the %s toggle in place while switching icons",
    async (os) => {
      platform.current = os;
      getTrayStatusMock.mockResolvedValue({ available: true, attempts: 0, lastError: null });
      document.cookie = "sidebar_state=false; path=/";
      render(<AppShell activeSection="home" onSectionChange={vi.fn()} />);
      const titleBar = screen.getByRole("banner");
      const toggle = screen.getByRole("button", { name: "Toggle Sidebar" });
      const position = toggle.parentElement?.className;
      expect(toggle.parentElement).toHaveClass(
        os === "macos" ? "left-[80px]" : "left-0",
        "top-[5px]",
      );
      expect(titleBar.querySelector(".lucide-panel-left-open")).toBeInTheDocument();
      await userEvent.click(toggle);
      expect(titleBar.querySelector(".lucide-panel-left-close")).toBeInTheDocument();
      expect(toggle.parentElement?.className).toBe(position);
      expect(document.cookie).toContain("sidebar_state=true");
    },
  );

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

  it("keeps the titlebar toggle accessible and routes sidebar navigation", async () => {
    getTrayStatusMock.mockResolvedValue({
      available: true,
      attempts: 0,
      lastError: null,
    });

    const onSectionChange = vi.fn();
    render(<AppShell activeSection="home" onSectionChange={onSectionChange} />);

    const titleBar = screen.getByRole("banner");
    expect(
      (
        document.querySelector('[data-slot="sidebar-wrapper"]') as HTMLElement
      ).style.getPropertyValue("--sidebar-width"),
    ).toBe("212px");
    expect(
      (
        document.querySelector('[data-slot="sidebar-wrapper"]') as HTMLElement
      ).style.getPropertyValue("--sidebar-width-icon"),
    ).toBe("76px");
    expect(titleBar).toHaveAttribute("data-tauri-drag-region");
    expect(screen.getByRole("button", { name: "Toggle Sidebar" })).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveTextContent("home");
    await userEvent.click(screen.getByRole("button", { name: "History" }));
    expect(onSectionChange).toHaveBeenCalledWith("history");
  });
});
