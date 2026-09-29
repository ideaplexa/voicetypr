import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { SettingsTab } from "./SettingsTab";

vi.mock("@/components/sections/GeneralSettings", () => ({
  GeneralSettings: () => <div>General settings</div>,
}));

it("renders General settings without owning app events", () => {
  render(<SettingsTab />);
  expect(screen.getByText("General settings")).toBeInTheDocument();
});
