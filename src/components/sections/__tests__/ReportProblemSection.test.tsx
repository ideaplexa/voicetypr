import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockInstance } from "vitest";
import { ReportProblemSection } from "../ReportProblemSection";
import { buildReportBody, gatherManualReportData, submitManualReport } from "@/utils/crashReport";
import { toast } from "sonner";

vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock("@/contexts/SettingsContext", () => ({
  useSettings: () => ({
    settings: { current_model: "base.en" },
  }),
}));

vi.mock("@/contexts/ModelManagementContext", () => ({
  useModelManagementContext: () => ({
    models: {
      "base.en": {
        display_name: "Base English",
      },
    },
  }),
}));

vi.mock("@/utils/crashReport", () => ({
  gatherManualReportData: vi.fn(),
  buildReportBody: vi.fn(),
  submitManualReport: vi.fn(),
}));

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("2.1.0-beta.2") }));

let writeTextMock: MockInstance<(data: string) => Promise<void>>;

const reportData = {
  message: "The app broke",
  appVersion: "1.0.0",
  platform: "windows",
  osVersion: "11",
  architecture: "x86_64",
  currentModel: "base.en",
  deviceId: "device-123",
  timestamp: "2026-04-27T00:00:00.000Z",
  logFileName: "voicetypr-2026-04-27.log",
  logContent: "INFO log line",
  logTruncated: false,
  logStatusNote: "",
  debugRingContent: "",
};

async function fillRequiredReportFields(
  user: ReturnType<typeof userEvent.setup>,
  message: string,
): Promise<void> {
  await user.type(screen.getByLabelText("What were you doing, and what went wrong?"), message);
}

describe("ReportProblemSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(gatherManualReportData).mockResolvedValue(reportData);
    vi.mocked(buildReportBody).mockReturnValue("REPORT BODY with The app broke");
    vi.mocked(submitManualReport).mockResolvedValue({ success: true, message: "Report submitted" });
    if (!navigator.clipboard) {
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: { writeText: async () => undefined },
      });
    }
    writeTextMock = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);
  });

  it("requires issue details without asking for name or email", async () => {
    const user = userEvent.setup();
    render(<ReportProblemSection />);
    expect(screen.getAllByRole("textbox")).toHaveLength(1);
    expect(screen.queryByLabelText(/email/i)).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/name/i)).not.toBeInTheDocument();
    expect(screen.getByRole("textbox")).toBeRequired();
    await user.click(screen.getByRole("button", { name: /send report/i }));
    expect(screen.getByText(/please describe the issue/i)).toBeInTheDocument();
    expect(gatherManualReportData).not.toHaveBeenCalled();
    expect(submitManualReport).not.toHaveBeenCalled();
  });

  it("shows Help and the automatic diagnostics disclosure without a logs switch", () => {
    render(<ReportProblemSection />);
    expect(screen.getByRole("heading", { name: "Help & feedback" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Report a problem" })).toBeInTheDocument();
    expect(screen.getByText(/Automatically includes app version/)).toBeInTheDocument();
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
    expect(screen.queryByText(/discord/i)).not.toBeInTheDocument();
  });

  it("opens Troubleshooting and Shortcuts in Settings", async () => {
    const user = userEvent.setup();
    const navigate = vi.fn();
    render(<ReportProblemSection onNavigateSettingsPane={navigate} />);
    await user.click(screen.getByRole("button", { name: "Troubleshooting" }));
    expect(navigate).toHaveBeenLastCalledWith("advanced");
    await user.click(screen.getByRole("button", { name: "Shortcuts" }));
    expect(navigate).toHaveBeenLastCalledWith("shortcuts");
  });

  it("shows the installed version and opens the existing update announcement", async () => {
    const user = userEvent.setup();
    render(<ReportProblemSection />);
    expect(await screen.findByText("See what changed in 2.1.0-beta.2.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "What's new" }));
    expect(screen.getByRole("dialog", { name: "Voicetypr Updated" })).toBeInTheDocument();
    expect(screen.getByText("Successfully updated to version 2.1.0-beta.2")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("submits diagnostics without name or email and clears the form", async () => {
    const user = userEvent.setup();
    render(<ReportProblemSection />);

    const issue = screen.getByLabelText("What were you doing, and what went wrong?");
    await fillRequiredReportFields(user, "The app broke");
    await user.click(screen.getByRole("button", { name: /send report/i }));

    await waitFor(() => expect(submitManualReport).toHaveBeenCalledTimes(1));
    expect(gatherManualReportData).toHaveBeenCalledWith(
      undefined,
      undefined,
      "The app broke",
      "Base English",
    );
    expect(submitManualReport).toHaveBeenCalledWith(
      expect.objectContaining({
        message: "The app broke",
        logContent: "INFO log line",
      }),
    );
    expect(issue).toHaveValue("");
    expect(toast.success).toHaveBeenCalledWith("Report submitted. Thank you.");
    expect(screen.getByRole("status")).toHaveTextContent("Report submitted. Thank you.");
    expect(screen.queryByText(/ID VT-/)).not.toBeInTheDocument();
    const submitted = vi.mocked(submitManualReport).mock.calls[0][0];
    expect(submitted).not.toHaveProperty("name");
    expect(submitted).not.toHaveProperty("email");
  });

  it("preserves the issue when diagnostic collection fails", async () => {
    const user = userEvent.setup();
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(gatherManualReportData).mockRejectedValueOnce(new Error("invoke failed"));
    render(<ReportProblemSection />);

    const issue = screen.getByLabelText("What were you doing, and what went wrong?");
    await fillRequiredReportFields(user, "The app broke");
    await user.click(screen.getByRole("button", { name: /send report/i }));

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Failed to gather report data"));
    expect(issue).toHaveValue("The app broke");
    expect(submitManualReport).not.toHaveBeenCalled();
  });

  it("offers the complete prepared report when direct submission fails", async () => {
    const user = userEvent.setup();
    vi.mocked(submitManualReport).mockResolvedValueOnce({
      success: false,
      message: "Too many reports. Please try again later.",
    });
    render(<ReportProblemSection />);

    await fillRequiredReportFields(user, "Copy this report");
    await user.click(screen.getByRole("button", { name: /send report/i }));

    const copyButton = await screen.findByRole("button", { name: /copy report/i });
    expect(copyButton).toBeEnabled();
    expect(screen.getByText("Report not sent")).toBeInTheDocument();
    expect(screen.getByText(/copy the prepared report/i)).toBeInTheDocument();

    await user.click(copyButton);
    expect(buildReportBody).toHaveBeenCalledWith(reportData);
    await waitFor(() =>
      expect(writeTextMock).toHaveBeenCalledWith("REPORT BODY with The app broke"),
    );
  });

  it("ignores a stale clipboard completion after retrying submission", async () => {
    const user = userEvent.setup();
    let resolveClipboard: (() => void) | undefined;
    writeTextMock.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveClipboard = resolve;
        }),
    );
    vi.mocked(submitManualReport).mockResolvedValue({
      success: false,
      message: "Submission unavailable.",
    });
    render(<ReportProblemSection />);

    await fillRequiredReportFields(user, "Retry this report");
    await user.click(screen.getByRole("button", { name: /send report/i }));
    await user.click(await screen.findByRole("button", { name: /copy report/i }));
    expect(writeTextMock).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: /send report/i }));
    await waitFor(() => expect(submitManualReport).toHaveBeenCalledTimes(2));
    await screen.findByRole("button", { name: /copy report/i });

    await act(async () => {
      resolveClipboard?.();
      await Promise.resolve();
    });
    expect(toast.success).not.toHaveBeenCalledWith("Report copied to clipboard");
  });

  it("still submits when the latest log is unavailable", async () => {
    const user = userEvent.setup();
    vi.mocked(gatherManualReportData).mockResolvedValueOnce({
      ...reportData,
      message: "No log case",
      logFileName: null,
      logContent: "",
      logStatusNote: "No log file found.",
    });
    render(<ReportProblemSection />);

    await fillRequiredReportFields(user, "No log case");
    await user.click(screen.getByRole("button", { name: /send report/i }));

    await waitFor(() => expect(submitManualReport).toHaveBeenCalledTimes(1));
    expect(submitManualReport).toHaveBeenCalledWith(
      expect.objectContaining({
        logFileName: null,
        logStatusNote: "No log file found.",
      }),
    );
  });
});
