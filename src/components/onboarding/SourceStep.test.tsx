import { render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { SourceStep } from "@/components/onboarding/SourceStep";

it("keeps every phase-one source card's title above its description", () => {
  render(<SourceStep sourceType="local" modelSize="2 GB" onConfirmSource={vi.fn()} />);
  const cards = screen.getAllByRole("button");
  expect(cards).toHaveLength(3);
  for (const card of cards) {
    const title = within(card).getByText(card.getAttribute("aria-label")!);
    const text = title.parentElement!;
    expect(card).toHaveClass("flex-row");
    expect(text).toHaveClass("flex", "flex-col");
    expect(text.children).toHaveLength(2);
    expect(text.firstElementChild).toBe(title);
    expect(text.lastElementChild).toHaveClass("text-[12.5px]", "font-normal", "text-muted-foreground");
  }
});
