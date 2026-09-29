import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { KeyCaps } from "./KeyCaps";

describe("KeyCaps sizes", () => {
  it("renders large caps for the Home hero and compact caps by default", () => {
    const { rerender } = render(<KeyCaps caps={["⌘"]} size="lg" />);
    expect(screen.getByText("⌘")).toHaveClass("text-[24px]", "px-3");
    rerender(<KeyCaps caps={["⌘"]} size="sm" />);
    expect(screen.getByText("⌘")).toHaveClass("text-xs", "px-2.5");
  });
});
