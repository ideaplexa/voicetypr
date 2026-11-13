import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { PolishContextSetting } from "@/components/polish/PolishContextSetting";

it("discloses provider sharing and delivers opt-out and opt-in changes", () => {
  const onCheckedChange = vi.fn();
  const { rerender } = render(<PolishContextSetting checked onCheckedChange={onCheckedChange} />);
  expect(screen.getByText("When Polish is on, text before your cursor and your selection are sent to your chosen Polish provider, including cloud providers. Voicetypr does not save this context.")).toBeTruthy();
  const toggle = screen.getByRole("switch", { name: "Use nearby text" });
  expect(toggle.getAttribute("aria-checked")).toBe("true");
  fireEvent.click(toggle);
  expect(onCheckedChange).toHaveBeenLastCalledWith(false, expect.anything());
  rerender(<PolishContextSetting checked={false} onCheckedChange={onCheckedChange} />);
  expect(toggle.getAttribute("aria-checked")).toBe("false");
  fireEvent.click(toggle);
  expect(onCheckedChange).toHaveBeenLastCalledWith(true, expect.anything());
});
