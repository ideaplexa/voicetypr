import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import { SettingsTab } from "./SettingsTab";

const checkForUpdatesManually = vi.hoisted(() => vi.fn().mockResolvedValue(undefined));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("2.1.0") }));
vi.mock("@/services/updateService", () => ({ updateService: { checkForUpdatesManually } }));
vi.mock("@/components/sections/GeneralSettings", () => ({
  GeneralSettings: () => <p>Appearance and app behavior</p>,
}));
vi.mock("@/components/sections/ShortcutsSection", () => ({
  ShortcutsSection: () => <p>Shortcut controls</p>,
}));
vi.mock("@/components/sections/AdvancedSection", () => ({
  AdvancedSection: () => <p>Quick fixes and reset</p>,
}));
vi.mock("@/components/sections/NetworkSharingCard", () => ({
  NetworkSharingCard: () => <p>Network sharing controls</p>,
}));
vi.mock("@/components/sections/AgentCliSection", () => ({
  AgentCliSection: () => <p>CLI and API controls</p>,
}));

it("opens General by default and checks for updates from About", async () => {
  const user = userEvent.setup();
  render(<SettingsTab pane="general" onPaneChange={vi.fn()} />);
  expect(screen.getByRole("heading", { name: "Settings" })).toBeInTheDocument();
  expect(screen.getByText("Appearance and app behavior")).toBeInTheDocument();
  expect(await screen.findByText("Voicetypr 2.1.0")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(checkForUpdatesManually).toHaveBeenCalledOnce();
});

it("keeps every advanced pane reachable", async () => {
  const user = userEvent.setup();
  const PaneHarness = () => {
    const [pane, setPane] = useState<Parameters<typeof SettingsTab>[0]["pane"]>("shortcuts");
    return <SettingsTab pane={pane} onPaneChange={setPane} />;
  };
  render(<PaneHarness />);
  const panes = screen.getByRole("navigation", { name: "Settings panes" });
  expect(screen.getByText("Shortcut controls")).toBeInTheDocument();
  expect(panes).toHaveTextContent("Advanced");
  expect(screen.getByText("Advanced")).toHaveClass("text-muted-foreground");
  for (const [label, content] of [
    ["Network sharing", "Network sharing controls"],
    ["CLI & API", "CLI and API controls"],
    ["Troubleshooting", "Quick fixes and reset"],
  ] as const) {
    await user.click(screen.getByRole("button", { name: label }));
    expect(screen.getByText(content)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: label })).toHaveAttribute("aria-current", "page");
  }
});
