import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { WhereYouTalk } from "@/components/insights/WhereYouTalk";

it("normalizes bars to the top app while keeping the real shares", () => {
  const { container, rerender } = render(
    <WhereYouTalk
      apps={[
        { name: "Notes", words: 75 },
        { name: "Mail", words: 25 },
      ]}
      loading={false}
    />,
  );
  const bars = container.querySelectorAll<HTMLElement>('[data-pencil-name="Bar"]');
  expect(bars[0].style.width).toBe("100%");
  expect(parseFloat(bars[1].style.width)).toBeCloseTo(100 / 3);
  expect(screen.getByText("75%")).toBeInTheDocument();
  expect(screen.getByText("25%")).toBeInTheDocument();
  expect(screen.getByText("Notes")).toHaveClass("w-[128px]", "truncate");
  const tileColor = screen.getByText("N").style.background;
  rerender(
    <WhereYouTalk
      apps={[
        { name: "Notes", words: 5 },
        { name: "Mail", words: 95 },
      ]}
      loading={false}
    />,
  );
  expect(screen.getByText("N").style.background).toBe(tileColor);
});
