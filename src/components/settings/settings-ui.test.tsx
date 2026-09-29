import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ChoiceCard, Segmented } from "./settings-ui";

describe("settings choices", () => {
  it("selects a segment with the keyboard and reports selection", async () => {
    const user = userEvent.setup();
    const changed = vi.fn();
    const { rerender } = render(
      <Segmented
        label="Theme"
        value="system"
        onValueChange={changed}
        options={[
          { value: "system", label: "System" },
          { value: "light", label: "Light" },
          { value: "dark", label: "Dark" },
        ]}
      />,
    );
    expect(screen.getByRole("button", { name: "System" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "System" }).className).toContain("data-pressed:bg-card");
    act(() => screen.getByRole("button", { name: "Light" }).focus());
    await user.keyboard("{Enter}");
    expect(changed).toHaveBeenCalledWith("light");
    rerender(
      <Segmented
        label="Theme"
        value="light"
        onValueChange={changed}
        options={[
          { value: "system", label: "System" },
          { value: "light", label: "Light" },
          { value: "dark", label: "Dark" },
        ]}
      />,
    );
    expect(screen.getByRole("button", { name: "Light" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "System" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("selects a choice card with Enter and exposes aria-selected", async () => {
    const user = userEvent.setup();
    const selected = vi.fn();
    render(
      <div role="listbox" aria-label="Engine">
        <ChoiceCard
          label="On this computer"
          description="Private transcription"
          selected={false}
          onSelect={selected}
        />
        <ChoiceCard label="Cloud" selected onSelect={vi.fn()} />
      </div>,
    );
    expect(screen.getByRole("option", { name: /Cloud/ })).toHaveAttribute("aria-selected", "true");
    screen.getByRole("option", { name: /On this computer/ }).focus();
    await user.keyboard("{Enter}");
    expect(selected).toHaveBeenCalledOnce();
  });
});
