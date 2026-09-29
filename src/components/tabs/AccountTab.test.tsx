import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { AccountTab } from "./AccountTab";

vi.mock("@/components/sections/AccountSection", () => ({
  AccountSection: () => <div>License settings</div>,
}));

it("renders License settings without owning app events", () => {
  render(<AccountTab />);
  expect(screen.getByText("License settings")).toBeInTheDocument();
});
