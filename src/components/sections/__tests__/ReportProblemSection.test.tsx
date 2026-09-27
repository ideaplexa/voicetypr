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

let writeTextMock: MockInstance<(data: string) => Promise<void>>;

const reportData = {
  name: "Jordan Lee",
  email: "jordan@example.com",
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
  await user.type(screen.getByLabelText("Email"), "jordan@example.com");
  await user.type(screen.getByLabelText("Describe the issue"), message);
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

  it("requires contact email and issue details while keeping name optional", async () => {
    const user = userEvent.setup();
    render(<ReportProblemSection />);

    expect(screen.getAllByRole("textbox")).toHaveLength(3);
    expect(screen.getByLabelText("Name (optional)")).not.toBeRequired();
    expect(screen.getByLabelText("Email")).toBeRequired();
    expect(screen.getByLabelText("Describe the issue")).toBeRequired();
    expect(screen.queryByRole("button", { name: /copy report/i })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /send report/i }));

    expect(screen.getByText(/enter an email address so we can follow up/i)).toBeInTheDocument();
    expect(screen.getByText(/please describe the issue/i)).toBeInTheDocument();
    expect(gatherManualReportData).not.toHaveBeenCalled();
    expect(submitManualReport).not.toHaveBeenCalled();
  });

  it("shows the report form and automatic attachment disclosure", () => {
    render(<ReportProblemSection />);

    expect(screen.getByRole("heading", { name: "Report a problem" })).toBeInTheDocument();
    expect(
      screen.getByText(
        "Tell us what happened and how to reach you. We'll attach the app version, your current model, system details, and recent diagnostic logs automatically.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Report details" })).toBeInTheDocument();
    expect(screen.getByLabelText("Name (optional)")).toBeInTheDocument();
    expect(screen.getByLabelText("Email")).toBeInTheDocument();
    expect(screen.getByLabelText("Describe the issue")).toBeInTheDocument();
    expect(screen.queryByText("Try a quick fix first")).not.toBeInTheDocument();
    expect(screen.queryByText("Quick fixes")).not.toBeInTheDocument();
    expect(screen.queryByText("System configuration included")).not.toBeInTheDocument();
  });

  it("rejects an invalid contact email", async () => {
    const user = userEvent.setup();
    render(<ReportProblemSection />);

    await user.type(screen.getByLabelText("Email"), "not-an-email");
    await user.type(screen.getByLabelText("Describe the issue"), "The app broke");
    await user.click(screen.getByRole("button", { name: /send report/i }));

    expect(screen.getByText("Enter a valid email address.")).toBeInTheDocument();
    expect(gatherManualReportData).not.toHaveBeenCalled();
  });

  it("submits contact details with diagnostics and clears the form", async () => {
    const user = userEvent.setup();
    render(<ReportProblemSection />);

    const name = screen.getByLabelText("Name (optional)");
    const email = screen.getByLabelText("Email");
    const issue = screen.getByLabelText("Describe the issue");
    await user.type(name, "Jordan Lee");
    await fillRequiredReportFields(user, "The app broke");
    await user.click(screen.getByRole("button", { name: /send report/i }));

    await waitFor(() => expect(submitManualReport).toHaveBeenCalledTimes(1));
    expect(gatherManualReportData).toHaveBeenCalledWith(
      "Jordan Lee",
      "jordan@example.com",
      "The app broke",
      "Base English",
    );
    expect(submitManualReport).toHaveBeenCalledWith(
      expect.objectContaining({
        email: "jordan@example.com",
        message: "The app broke",
        logContent: "INFO log line",
      }),
    );
    expect(name).toHaveValue("");
    expect(email).toHaveValue("");
    expect(issue).toHaveValue("");
    expect(toast.success).toHaveBeenCalledWith("Report submitted. Thank you.");
  });

  it("preserves contact details and the issue when diagnostic collection fails", async () => {
    const user = userEvent.setup();
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.mocked(gatherManualReportData).mockRejectedValueOnce(new Error("invoke failed"));
    render(<ReportProblemSection />);

    const name = screen.getByLabelText("Name (optional)");
    const email = screen.getByLabelText("Email");
    const issue = screen.getByLabelText("Describe the issue");
    await user.type(name, "Jordan Lee");
    await fillRequiredReportFields(user, "The app broke");
    await user.click(screen.getByRole("button", { name: /send report/i }));

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Failed to gather report data"));
    expect(name).toHaveValue("Jordan Lee");
    expect(email).toHaveValue("jordan@example.com");
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
