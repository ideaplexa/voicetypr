import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { homeNeeds } from "@/lib/home-needs";
import { NeedsYou } from "@/components/tabs/overview/NeedsAndTips";

it.each([true, false])("offers retry, available alternatives, and deletion after retry fails: alternative=%s", async (alternative) => {
  const rows = homeNeeds({
    kept: [{ id: "kept", engine_short: "Soniox", alt_engine_short: alternative ? "Whisper" : null, alt_engine_id: alternative ? "small.en" : null, remaining_ms: 6 * 86400000, busy: false, retry_failed: true }],
    updateVersion: null, status: { label: "Ready", ready: true }, accessibilityMissing: false,
  });
  const act = vi.fn();
  render(<NeedsYou rows={rows} onAct={act} />);
  const user = userEvent.setup();
  expect(screen.getByText("Soniox couldn't transcribe this")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /^Retry again:/ }));
  expect(act.mock.lastCall?.[0].action).toEqual({ kind: "retry", id: "kept", engine: null });
  if (alternative) {
    await user.click(screen.getByRole("button", { name: /^Try another model:/ }));
    expect(act.mock.lastCall?.[0].action).toEqual({ kind: "retry", id: "kept", engine: "small.en" });
  } else expect(screen.queryByRole("button", { name: /^Try another model:/ })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /^Delete audio:/ }));
  expect(act.mock.lastCall?.[0].action).toEqual({ kind: "discard", id: "kept" });
});
