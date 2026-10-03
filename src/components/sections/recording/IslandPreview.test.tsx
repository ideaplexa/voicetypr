import { render } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { IslandPreview } from "@/components/sections/recording/IslandPreview";
it("renders the listening island at the chosen position without scheduling frames", () => {
  const canvas = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  const frames = vi.spyOn(window, "requestAnimationFrame");
  const {getByRole, unmount} = render(<IslandPreview position="top-right" />);
  const root = getByRole("img", {hidden:true}).shadowRoot!;
  expect(root.querySelector('.pill-root')).toHaveAttribute('data-pill-position','top-right');
  expect(root.querySelector('.listening-row')).not.toHaveAttribute('hidden');
  expect(root.querySelector('.letter')).toHaveTextContent('A');
  expect(frames).not.toHaveBeenCalled();
  unmount(); frames.mockRestore(); canvas.mockRestore();
});
