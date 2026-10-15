import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SidebarStatusBar } from "./SidebarStatusBar";
import { TooltipProvider } from "@/components/ui/tooltip";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("2.1.0-beta.5") }));

type Status = { checking: boolean; available: { version: string } | null; lastChecked: number | null; result: "idle" | "current" | "error" | "store"; install: "idle" | "prompting" | "installing" | "restart_pending"; restartVersion: string | null };
const update = vi.hoisted(() => ({
  status: { checking: false, available: null, lastChecked: null, result: "idle", install: "idle", restartVersion: null } as Status,
  checkNow: vi.fn(),
  promptInstall: vi.fn(),
  restartNow: vi.fn(),
}));
vi.mock("@/hooks/useUpdateStatus", () => ({
  useUpdateStatus: () => ({ ...update.status, checkNow: update.checkNow, promptInstall: update.promptInstall, restartNow: update.restartNow }),
}));

const license = { label: "Pro", color: "text-sage" };
const renderBar = (collapsed = false) => {
  const onOpenLicense = vi.fn();
  const view = render(
    <TooltipProvider>
      <SidebarStatusBar license={license} collapsed={collapsed} onOpenLicense={onOpenLicense} />
    </TooltipProvider>,
  );
  return { onOpenLicense, ...view };
};

beforeEach(() => {
  update.status = { checking: false, available: null, lastChecked: null, result: "idle", install: "idle", restartVersion: null };
  update.checkNow.mockReset();
  update.promptInstall.mockReset();
  update.restartNow.mockReset();
});

describe("Sidebar status bar", () => {
  it("shows the licence as plain text and the version in mono, and opens License", async () => {
    const { onOpenLicense } = renderBar();
    expect(await screen.findByText("2.1.0-beta.5")).toHaveClass("font-mono");
    const chip = screen.getByRole("button", { name: "Pro. Open License" });
    expect(chip).not.toHaveClass("bg-card", "ring-1");
    await userEvent.click(chip);
    expect(onOpenLicense).toHaveBeenCalledOnce();
  });

  it("idle: checks for updates, then briefly says Up to date without a modal", async () => {
    update.checkNow.mockResolvedValue({ ...update.status, result: "current" });
    renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(update.checkNow).toHaveBeenCalledOnce();
    expect(await screen.findByText("Up to date")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(update.promptInstall).not.toHaveBeenCalled();
  });

  it("checking: the icon spins and the button is disabled", () => {
    update.status = { ...update.status, checking: true };
    renderBar();
    const button = screen.getByRole("button", { name: "Checking for updates" });
    expect(button).toBeDisabled();
    expect(button.querySelector("svg")).toHaveClass("animate-spin");
  });

  it("available: becomes a download badge that opens the install dialog", async () => {
    update.status = { ...update.status, available: { version: "2.1.0-beta.6" } };
    renderBar();
    expect(screen.queryByRole("button", { name: "Check for updates" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Update 2.1.0-beta.6 available. Install/ }));
    expect(update.promptInstall).toHaveBeenCalledOnce();
  });

  it("walks idle to checking to available through one check", async () => {
    let finish: (value: Status) => void = () => {};
    update.checkNow.mockImplementation(() => new Promise<Status>((resolve) => { finish = resolve; }));
    const view = renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    update.status = { ...update.status, checking: true };
    view.rerender(<TooltipProvider><SidebarStatusBar license={license} collapsed={false} onOpenLicense={vi.fn()} /></TooltipProvider>);
    expect(screen.getByRole("button", { name: "Checking for updates" })).toBeDisabled();
    update.status = { checking: false, available: { version: "2.1.0-beta.6" }, lastChecked: 1, result: "current", install: "idle", restartVersion: null };
    await act(async () => finish(update.status));
    view.rerender(<TooltipProvider><SidebarStatusBar license={license} collapsed={false} onOpenLicense={vi.fn()} /></TooltipProvider>);
    await waitFor(() => expect(screen.getByRole("button", { name: /Install/ })).toBeInTheDocument());
    expect(screen.queryByText("Up to date")).not.toBeInTheDocument();
  });

  it("does not say Up to date when the check has not finished or the result is unknown", async () => {
    update.checkNow.mockResolvedValue({ ...update.status, checking: true, result: "idle" });
    renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    await waitFor(() => expect(update.checkNow).toHaveBeenCalled());
    expect(screen.queryByText("Up to date")).not.toBeInTheDocument();
  });

  it("reports a failed check instead of Up to date", async () => {
    update.checkNow.mockResolvedValue({ ...update.status, result: "error" });
    renderBar();
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Check failed")).toBeInTheDocument();
    expect(screen.queryByText("Up to date")).not.toBeInTheDocument();
  });

  it("installing: the badge is disabled; restart pending: it offers Restart", async () => {
    update.status = { ...update.status, available: { version: "2.1.0-beta.6" }, install: "installing" };
    const { unmount } = renderBar();
    expect(screen.getByRole("button", { name: /Installing update/ })).toBeDisabled();
    unmount();
    update.status = { ...update.status, available: null, install: "restart_pending", restartVersion: "2.1.0-beta.6" };
    renderBar();
    await userEvent.click(screen.getByRole("button", { name: /Restart/ }));
    expect(update.restartNow).toHaveBeenCalledOnce();
    expect(update.promptInstall).not.toHaveBeenCalled();
  });

  it("collapsed: keeps the licence icon and only surfaces an available update", () => {
    const { unmount } = renderBar(true);
    expect(screen.getByRole("button", { name: "Pro. Open License" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Check for updates" })).not.toBeInTheDocument();
    unmount();
    update.status = { ...update.status, available: { version: "2.1.0-beta.6" } };
    renderBar(true);
    expect(screen.getByRole("button", { name: /Install/ })).toBeInTheDocument();
  });
});
