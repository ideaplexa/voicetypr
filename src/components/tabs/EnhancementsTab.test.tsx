import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { EnhancementsTab } from "./EnhancementsTab";

vi.mock("@/components/sections/EnhancementsSection", () => ({
  EnhancementsSection: () => <div>Polish settings</div>,
}));

it("renders Polish settings without owning app events", () => {
  render(<EnhancementsTab />);
  expect(screen.getByText("Polish settings")).toBeInTheDocument();
});
