import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { KeepWordsSetting } from "@/components/polish/KeepWordsSetting";

it("labels the controlled switch and delivers toggles in both directions", () => {
  const onCheckedChange = vi.fn();
  const { rerender } = render(<KeepWordsSetting checked={false} onCheckedChange={onCheckedChange} />);
  expect(screen.getByText("Only fix punctuation, fillers and stutters — never reword.")).toBeTruthy();
  const toggle = screen.getByRole("switch", { name: "Keep my words" });
  expect(toggle.getAttribute("aria-checked")).toBe("false");
  fireEvent.click(toggle);
  expect(onCheckedChange).toHaveBeenLastCalledWith(true, expect.anything());
  rerender(<KeepWordsSetting checked onCheckedChange={onCheckedChange} />);
  expect(toggle.getAttribute("aria-checked")).toBe("true");
  fireEvent.click(toggle);
  expect(onCheckedChange).toHaveBeenLastCalledWith(false, expect.anything());
});
