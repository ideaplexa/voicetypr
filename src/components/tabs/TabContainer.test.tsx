import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { TabContainer } from "./TabContainer";

// Mock Tauri API
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

// Mock event coordinator hook
vi.mock("@/hooks/useEventCoordinator", () => ({
  useEventCoordinator: () => ({
    registerEvent: vi.fn(),
    unregisterEvent: vi.fn(),
  }),
}));

// Mock all tab components with simple test versions
vi.mock("./RecordingsTab", () => ({
  RecordingsTab: () => <div data-testid="recordings-tab">Recordings</div>,
}));

vi.mock("./RecordingTab", () => ({
  RecordingTab: () => <div data-testid="recording-tab">Recording</div>,
}));

vi.mock("./OverviewTab", () => ({
  OverviewTab: () => <div data-testid="overview-tab">Overview</div>,
}));

vi.mock("./ModelsTab", () => ({
  ModelsTab: ({ sourceFilter }: { sourceFilter?: string }) => (
    <div data-testid="models-tab">Models {sourceFilter}</div>
  ),
}));

vi.mock("./SettingsTab", () => ({
  SettingsTab: () => <div data-testid="settings-tab">Settings</div>,
}));

vi.mock("./EnhancementsTab", () => ({
  EnhancementsTab: () => <div data-testid="enhancements-tab">Enhancements</div>,
}));

vi.mock("./AdvancedTab", () => ({
  AdvancedTab: () => <div data-testid="advanced-tab">Advanced</div>,
}));

vi.mock("./AccountTab", () => ({
  AccountTab: () => <div data-testid="account-tab">Account</div>,
}));

vi.mock("../sections/ReportProblemSection", () => ({
  ReportProblemSection: () => <div data-testid="report-problem-tab">Report problem</div>,
}));

describe("TabContainer", () => {
  it("passes the saved Cloud destination when mounting Sources from another tab", () => {
    const { rerender } = render(<TabContainer activeSection="overview" sourceFilter="local" />);
    expect(screen.queryByTestId("models-tab")).not.toBeInTheDocument();
    rerender(<TabContainer activeSection="models" sourceFilter="cloud" />);
    expect(screen.getByTestId("models-tab")).toHaveTextContent("Models cloud");
    rerender(<TabContainer activeSection="models" sourceFilter="remote" />);
    expect(screen.getByTestId("models-tab")).toHaveTextContent("Models remote");
    rerender(<TabContainer activeSection="models" sourceFilter="cloud" />);
    expect(screen.getByTestId("models-tab")).toHaveTextContent("Models cloud");
  });

  it("renders correct tab based on activeSection", () => {
    const { rerender } = render(<TabContainer activeSection="overview" />);
    expect(screen.getByTestId("overview-tab")).toBeInTheDocument();

    rerender(<TabContainer activeSection="recordings" />);
    expect(screen.getByTestId("recordings-tab")).toBeInTheDocument();

    rerender(<TabContainer activeSection="recording" />);
    expect(screen.getByTestId("recording-tab")).toBeInTheDocument();

    rerender(<TabContainer activeSection="models" />);
    expect(screen.getByTestId("models-tab")).toBeInTheDocument();

    rerender(<TabContainer activeSection="general" />);
    expect(screen.getByTestId("settings-tab")).toBeInTheDocument();

    rerender(<TabContainer activeSection="formatting" />);
    expect(screen.getByTestId("enhancements-tab")).toBeInTheDocument();

    rerender(<TabContainer activeSection="advanced" />);
    expect(screen.getByTestId("advanced-tab")).toBeInTheDocument();

    rerender(<TabContainer activeSection="license" />);
    expect(screen.getByTestId("account-tab")).toBeInTheDocument();

    rerender(<TabContainer activeSection="report-problem" />);
    expect(screen.getByTestId("report-problem-tab")).toBeInTheDocument();
  });

  it("renders overview tab for unknown sections", () => {
    render(<TabContainer activeSection={"unknown" as unknown as never} />);
    expect(screen.getByTestId("overview-tab")).toBeInTheDocument();
  });
});
