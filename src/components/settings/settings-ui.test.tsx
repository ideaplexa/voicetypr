import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ChoiceCard, Segmented } from "./settings-ui";

describe("settings choices", () => {
  it.each(["push_to_talk", "toggle"])("raises the selected recording mode %s", (mode) => {
    render(<Segmented label="Recording mode" value={mode} onValueChange={vi.fn()} options={[
      { value: "push_to_talk", label: "Hold to talk" },
      { value: "toggle", label: "Press to start / stop" },
    ]} />);
    const selected = screen.getByRole("button", { name: mode === "toggle" ? "Press to start / stop" : "Hold to talk" });
    expect(selected).toHaveAttribute("aria-pressed", "true");
    expect(selected.className).toContain("aria-pressed:bg-card!");
    expect(selected.className).toContain("aria-pressed:shadow-sm");
    expect(selected.className).toContain("aria-pressed:font-semibold");
    expect(selected.className).toContain("dark:aria-pressed:border-");
  });
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
    expect(screen.getByRole("button", { name: "System" }).className).toContain(
      "aria-pressed:bg-card!",
    );
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
    expect(screen.getByRole("group", { name: "Theme" })).not.toHaveAttribute("data-spacing", "0");
    expect(screen.getByRole("button", { name: "Light" }).className).toContain("rounded-[7px]");
    expect(screen.getByRole("button", { name: "System" })).toHaveAttribute("aria-pressed", "false");
  });

  it("selects a choice card with Enter and exposes aria-pressed", async () => {
    const user = userEvent.setup();
    const selected = vi.fn();
    render(
      <div aria-label="Engine">
        <ChoiceCard
          label="On this computer"
          description="Private transcription"
          selected={false}
          onSelect={selected}
        />
        <ChoiceCard label="Cloud" selected onSelect={vi.fn()} />
      </div>,
    );
    expect(screen.getByRole("button", { name: /Cloud/ })).toHaveAttribute("aria-pressed", "true");
    screen.getByRole("button", { name: /On this computer/ }).focus();
    await user.keyboard("{Enter}");
    expect(selected).toHaveBeenCalledOnce();
  });
});
