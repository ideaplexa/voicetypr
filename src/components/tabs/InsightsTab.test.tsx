import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { InsightsTab } from "@/components/tabs/InsightsTab";
import { createUsageFixture } from "@/ui-preview/usageFixture";
import { localDateKey, shiftDay } from "@/components/insights/stats";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({ settings: { hotkey: "CommandOrControl+Shift+Space" } }),
}));
vi.mock("@/hooks/useActiveTrigger", () => ({
  useActiveTrigger: () => ({ hotkey: "CommandOrControl+Shift+Space", kbdLabel: "⌘" }),
}));
vi.mock("@/components/ShareStatsModal", () => ({
  ShareStatsModal: ({ open }: { open: boolean }) => (open ? <div>Share modal opened</div> : null),
}));
beforeEach(() => {
  vi.mocked(invoke).mockImplementation(async (_command, args) =>
    createUsageFixture(
      args && "since" in args && typeof args.since === "string" ? args.since : null,
    ),
  );
});
describe("Insights", () => {
  it("renders the fixture, all sections and the share entry point", async () => {
    render(<InsightsTab />);
    expect(await screen.findByText("Words dictated")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Activity" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /Activity over 43 weeks/ })).toBeInTheDocument();
    expect(screen.getByText("Slack")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Milestones" })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_usage_stats", { since: null });
    await userEvent.click(screen.getByRole("button", { name: "Share" }));
    expect(screen.getByText("Share modal opened")).toBeInTheDocument();
  });
  it("re-fetches period app rankings with inclusive local since", async () => {
    render(<InsightsTab />);
    await screen.findByText("Words dictated");
    await userEvent.click(screen.getByRole("button", { name: "Week" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_usage_stats", {
        since: localDateKey(shiftDay(new Date(), -6)),
      }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Month" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_usage_stats", {
        since: localDateKey(shiftDay(new Date(), -29)),
      }),
    );
    await userEvent.click(screen.getByRole("button", { name: "All time" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("get_usage_stats", { since: null }),
    );
  });
  it("renders the empty state with shortcut keycaps", async () => {
    vi.mocked(invoke).mockResolvedValue(createUsageFixture(null, true));
    render(<InsightsTab />);
    expect(
      await screen.findByText("Your stats appear after your first dictation"),
    ).toBeInTheDocument();
    expect(screen.getByText("Space")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Share" })).toBeDisabled();
  });
  it("hides pace without audio and shows apps empty state", async () => {
    vi.mocked(invoke).mockResolvedValue({ ...createUsageFixture(), total_audio_ms: 0, apps: [] });
    render(<InsightsTab />);
    await screen.findByText("Words dictated");
    expect(screen.queryByText("Pace")).not.toBeInTheDocument();
    expect(screen.getByText("App stats appear when you dictate into an app.")).toBeInTheDocument();
  });
  it("offers retry on command failure", async () => {
    let failed = false;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "record_observability_event") return;
      if (!failed) { failed = true; throw new Error("unavailable"); }
      return createUsageFixture();
    });
    render(<InsightsTab />);
    await screen.findByRole("alert");
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Words dictated")).toBeInTheDocument();
  });
});
