import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { EnhancementSettings } from "@/components/EnhancementSettings";
import { defaultWritingSettings } from "@/types/writing";

const rules = [
  { app_name: "Slack", preset: "Message", enabled: true },
  { app_name: "Cursor", preset: "Code", enabled: false },
] as const;

function renderStyles(disabled = false) {
  const change = vi.fn();
  render(<EnhancementSettings preset="CleanDictation" finalTextLanguage="same_as_transcript"
    writingSettings={{ ...defaultWritingSettings, app_formatting_rules: [...rules] }}
    aiFormattingEnabled providerContent={<p>Provider</p>} onPresetChange={vi.fn()}
    onFinalTextLanguageChange={vi.fn()} onWritingSettingsChange={change} writingSettingsDisabled={disabled} />);
  return change;
}

it("renders each app's name, visible style, switch and delete button together in one compact row", () => {
  renderStyles();
  rules.forEach((rule, index) => {
    const name = screen.getByRole("textbox", { name: `App name ${index + 1}` });
    const style = screen.getByRole("combobox", { name: `Style for ${rule.app_name}` });
    const enabled = screen.getByRole("switch", { name: `Enable ${rule.app_name} style` });
    const remove = screen.getByRole("button", { name: `Delete ${rule.app_name} style` });
    const row = name.parentElement!;
    expect(row).toHaveClass("flex", "items-center", "rounded-[8px]", "bg-muted", "px-2.5", "py-2");
    expect(row).not.toHaveClass("flex-wrap");
    expect(Array.from(row.children).filter((child) => child.getAttribute("aria-hidden") !== "true")).toEqual([name, style, enabled, remove]);
    expect(name).toHaveClass("min-w-0", "flex-1");
    expect(style).toHaveTextContent(rule.preset);
    expect(enabled).toHaveAttribute("aria-checked", String(rule.enabled));
  });
  expect(screen.queryByText("Enabled")).not.toBeInTheDocument();
});

it("retains app editing, style selection, enable and delete behavior", async () => {
  const change = renderStyles();
  const user = userEvent.setup();
  fireEvent.change(screen.getByRole("textbox", { name: "App name 1" }), { target: { value: "Mail" } });
  expect(change).toHaveBeenLastCalledWith({ app_formatting_rules: [{ ...rules[0], app_name: "Mail" }, rules[1]] });
  await user.click(screen.getByRole("combobox", { name: "Style for Slack" }));
  await user.click(screen.getByRole("option", { name: "Writing" }));
  expect(change).toHaveBeenLastCalledWith({ app_formatting_rules: [{ ...rules[0], preset: "Writing" }, rules[1]] });
  await user.click(screen.getByRole("switch", { name: "Enable Slack style" }));
  expect(change).toHaveBeenLastCalledWith({ app_formatting_rules: [{ ...rules[0], enabled: false }, rules[1]] });
  await user.click(screen.getByRole("button", { name: "Delete Slack style" }));
  expect(change).toHaveBeenLastCalledWith({ app_formatting_rules: [rules[1]] });
});

it("disables every app-rule control while settings are unavailable", () => {
  renderStyles(true);
  expect(screen.getByRole("textbox", { name: "App name 1" })).toBeDisabled();
  expect(screen.getByRole("combobox", { name: "Style for Slack" })).toBeDisabled();
  expect(screen.getByRole("switch", { name: "Enable Slack style" })).toHaveAttribute("aria-disabled", "true");
  expect(screen.getByRole("button", { name: "Delete Slack style" })).toBeDisabled();
});
