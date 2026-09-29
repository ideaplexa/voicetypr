import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { OverviewTab } from "./OverviewTab";

vi.mock("@/contexts/ReadinessContext", () => ({ useReadiness: () => ({ canRecord: true, remoteSelected: false }), useCanAutoInsert: () => true }));
vi.mock("@/contexts/SettingsContext", () => ({ useSettings: () => ({ settings: { hotkey: "Alt+Space", current_model: "parakeet-v3", speech_language: "en" } }) }));
vi.mock("@/hooks/useTranscriptionHistory", () => ({ useTranscriptionHistory: () => ({ history: [], totalCount: 0, isLoading: false, loadError: null, refreshHistory: vi.fn() }) }));
vi.mock("@/hooks/useActiveTrigger", () => ({ useActiveTrigger: () => ({ label: "⌥ Space" }) }));
vi.mock("./overview/useActiveRemoteLabel", () => ({ useActiveRemoteLabel: () => ({ label: null }) }));
vi.mock("./overview/CurrentSetupCard", () => ({ CurrentSetupCard: () => <div>Current setup</div> }));
vi.mock("./overview/WeeklyRhythmCard", () => ({ WeeklyRhythmCard: () => <div>Weekly rhythm</div> }));
vi.mock("@/components/ShareStatsModal", () => ({ ShareStatsModal: () => null }));

it("uses the Home page title and keeps its description", () => {
  render(<OverviewTab />);
  expect(screen.getByRole("heading", { name: "Home" })).toBeInTheDocument();
  expect(screen.getByText("Your active dictation setup and usage at a glance.")).toBeInTheDocument();
});
