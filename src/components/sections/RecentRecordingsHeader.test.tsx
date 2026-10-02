import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { RecentRecordingsHeader } from "@/components/sections/RecentRecordingsHeader";

it("keeps export and clear actions in the overflow menu", async () => {
  const onExportText = vi.fn();
  const onClearAll = vi.fn();
  render(
    <RecentRecordingsHeader
      historyLength={2}
      onExport={vi.fn()}
      onExportText={onExportText}
      onClearAll={onClearAll}
    />,
  );
  expect(screen.queryByRole("button", { name: "Clear all" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Export" })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "History actions" }));
  await userEvent.click(await screen.findByRole("menuitem", { name: "Export" }));
  await userEvent.click(await screen.findByRole("menuitem", { name: "Plain text (.txt)" }));
  expect(onExportText).toHaveBeenCalledWith("txt");
  await userEvent.click(screen.getByRole("button", { name: "History actions" }));
  await userEvent.click(await screen.findByRole("menuitem", { name: "Clear all" }));
  expect(onClearAll).toHaveBeenCalledOnce();
});
