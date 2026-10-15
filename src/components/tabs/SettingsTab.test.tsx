import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import { SettingsTab } from "./SettingsTab";

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ settings: {}, updateSettings: vi.fn().mockResolvedValue(undefined) }),
}));
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

it("lists seven panes in three groups, with About and Privacy folded away", () => {
  render(<SettingsTab pane="general" onPaneChange={vi.fn()} />);
  expect(screen.getByRole("heading", { name: "Settings" })).toBeInTheDocument();
  expect(screen.getByText("Appearance and app behavior")).toBeInTheDocument();
  const panes = within(screen.getByRole("navigation", { name: "Settings panes" }));
  expect(panes.getAllByRole("button").map((button) => button.textContent)).toEqual([
    "General",
    "App shortcuts",
    "Storage",
    "Network sharing",
    "CLI & API",
    "Troubleshooting",
    "License",
  ]);
  expect(panes.getByText("Advanced")).toBeInTheDocument();
  expect(panes.getByText("Account")).toBeInTheDocument();
  expect(panes.queryByRole("button", { name: /About|Privacy/ })).not.toBeInTheDocument();
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

it("keeps the selected pane card-colored while the pointer rests on it", async () => {
  const Harness = () => {
    const [pane, setPane] = useState<"general" | "shortcuts">("general");
    return (
      <SettingsTab
        pane={pane}
        onPaneChange={(next) => {
          if (next === "general" || next === "shortcuts") setPane(next);
        }}
      />
    );
  };
  render(<Harness />);
  const shortcuts = screen.getByRole("button", { name: "App shortcuts" });
  await userEvent.click(shortcuts);
  await userEvent.hover(shortcuts);
  expect(shortcuts).toHaveAttribute("aria-current", "page");
  expect(shortcuts).toHaveClass("bg-card", "hover:bg-card", "dark:bg-[#2A2A2D]");
  expect(shortcuts).not.toHaveClass("hover:bg-muted", "hover:bg-black/[0.04]");
  expect(screen.getByRole("button", { name: "General" })).toHaveClass("hover:bg-black/[0.04]");
});
