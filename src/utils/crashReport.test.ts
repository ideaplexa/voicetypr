import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  buildCrashReportPayload,
  buildManualReportPayload,
  buildReportBody,
  submitManualReport,
  submitCrashReport,
  type CrashReportData,
  type ManualReportData,
} from "./crashReport";

const baseReport: ManualReportData = {
  message: "The app failed after recording.",
  appVersion: "1.12.2",
  platform: "macos",
  osVersion: "15.0",
  architecture: "aarch64",
  currentModel: "base.en",
  deviceId: "device-123",
  timestamp: "2026-04-27T00:00:00.000Z",
  logFileName: "voicetypr-2026-04-27.log",
  logContent: "INFO redacted log line",
  logTruncated: false,
  logStatusNote: "",
  debugRingContent: "",
};

const baseCrashReport: CrashReportData = {
  errorMessage: "Boom",
  errorStack: "Error stack",
  componentStack: "Component stack",
  appVersion: "1.12.2",
  platform: "macos",
  osVersion: "15.0",
  architecture: "aarch64",
  currentModel: "base.en",
  deviceId: "device-123",
  timestamp: "2026-04-27T00:00:00.000Z",
  logFileName: "voicetypr-2026-04-27.log",
  logContent: "INFO redacted log line",
  logTruncated: false,
  logStatusNote: "",
  debugRingContent: "",
};

beforeEach(() => {
  vi.restoreAllMocks();
});

describe("buildReportBody", () => {
  it("omits the contact section when name and email are blank", () => {
    const body = buildReportBody(baseReport);

    expect(body).not.toContain("### Contact");
    expect(body).toContain("### Message");
    expect(body).toContain("The app failed after recording.");
  });

  it("formats environment and latest log sections", () => {
    const body = buildReportBody({
      ...baseReport,
      name: "Moin",
      email: "moin@example.com",
      logTruncated: true,
    });

    expect(body).toContain("### Contact");
    expect(body).toContain("Name: Moin");
    expect(body).toContain("Email: moin@example.com");
    expect(body).toContain("| App Version | 1.12.2 |");
    expect(body).toContain("| Device ID | device-123 |");
    expect(body).toContain("| Platform | macos |");
    expect(body).toContain("| Current Model | Base (English) |");
    expect(body).toContain("_The log was truncated. Only the most recent entries are included._");
    expect(body).toContain("_Source: voicetypr-2026-04-27.log_");
    expect(body).toContain("INFO redacted log line");
  });

  it("labels latest log status notes without log content", () => {
    const body = buildReportBody({
      ...baseReport,
      logFileName: null,
      logContent: "",
      logStatusNote: "No log file found.",
      debugRingContent: "",
    });

    expect(body).toContain("## Latest App Log");
    expect(body).toContain("> No log file found.");
  });

  it("renders the System section when systemSpecs are present", () => {
    const body = buildReportBody({
      ...baseReport,
      systemSpecs: {
        osName: "macOS",
        osVersion: "15.0",
        kernelVersion: "24.0.0",
        arch: "aarch64",
        cpuBrand: "Apple M4 Pro",
        cpuCores: 12,
        totalMemoryMb: 24576,
        gpus: ["Apple M4 Pro"],
      },
    });

    expect(body).toContain("## System");
    expect(body).toContain("| OS | macOS 15.0 |");
    expect(body).toContain("| Kernel | 24.0.0 |");
    expect(body).toContain("| CPU | Apple M4 Pro (12 cores) |");
    expect(body).toContain("| Memory | 24 GB |");
    expect(body).toContain("| GPU | Apple M4 Pro |");
  });

  it("includes unavailable tray diagnostics for support", () => {
    const body = buildReportBody({
      ...baseReport,
      trayStatus: {
        available: false,
        attempts: 8,
        lastError: "status | area\nunavailable",
      },
    });

    expect(body).toContain("| Menu-bar Icon | Unavailable |");
    expect(body).toContain("| Tray Creation Attempts | 8 |");
    expect(body).toContain("| Tray Creation Error | status area unavailable |");
  });

  it("shows a collection-failure note instead of silently omitting specs", () => {
    const body = buildReportBody({
      ...baseReport,
      systemSpecsError: "command get_system_specs panicked | in sysinfo",
    });
    expect(body).toContain("## System");
    expect(body).toContain(
      "> System configuration could not be collected: command get_system_specs panicked in sysinfo",
    );
  });

  it("omits the System section entirely when specs are absent with no error", () => {
    const body = buildReportBody(baseReport);
    expect(body).not.toContain("## System");
  });

  it("labels GPU as Unknown when no adapters were detected", () => {
    const body = buildReportBody({
      ...baseReport,
      systemSpecs: {
        osName: "Windows",
        osVersion: "11",
        kernelVersion: "10.0.22631",
        arch: "x86_64",
        cpuBrand: "Intel",
        cpuCores: 8,
        totalMemoryMb: 16384,
        gpus: [],
      },
    });

    expect(body).toContain("| GPU | Unknown |");
  });
});

describe("debug ring section", () => {
  it("attaches the in-memory ring when present", () => {
    const body = buildReportBody({
      ...baseReport,
      debugRingContent: "DEBUG REC TIMING start=12ms\nDEBUG WHISPER_BACKEND cpu",
    });

    expect(body).toContain("## Debug (most recent, in-memory ring)");
    expect(body).toContain("DEBUG WHISPER_BACKEND cpu");
    expect(body.indexOf("## Debug")).toBeGreaterThan(body.indexOf("## Latest App Log"));
  });

  it("omits the ring section when the ring is empty", () => {
    const body = buildReportBody(baseReport);
    expect(body).not.toContain("## Debug");
  });
});

describe("report submission payloads", () => {
  it("builds the manual report endpoint payload", () => {
    expect(buildManualReportPayload(baseReport)).toEqual({
      kind: "manual",
      message: "The app failed after recording.",
      environment: {
        appVersion: "1.12.2",
        platform: "macos",
        osVersion: "15.0",
        architecture: "aarch64",
        currentModel: "base.en",
        deviceId: "device-123",
        timestamp: "2026-04-27T00:00:00.000Z",
      },
      latestLog: {
        fileName: "voicetypr-2026-04-27.log",
        content: "INFO redacted log line",
        truncated: false,
        statusNote: "",
        debugRing: "",
      },
    });
  });

  it("ships the debug ring in the submitted payload (not just the copy view)", () => {
    const payload = buildManualReportPayload({
      ...baseReport,
      debugRingContent: "DEBUG WHISPER_BACKEND cpu",
    });
    expect(payload.latestLog.debugRing).toBe("DEBUG WHISPER_BACKEND cpu");
  });

  it("includes tray status in the support endpoint payload", () => {
    const payload = buildManualReportPayload({
      ...baseReport,
      trayStatus: {
        available: false,
        attempts: 5,
        lastError: "status area unavailable",
      },
    });

    expect(payload.environment.trayStatus).toEqual({
      available: false,
      attempts: 5,
      lastError: "status area unavailable",
    });
  });

  it("builds the crash report endpoint payload", () => {
    expect(buildCrashReportPayload(baseCrashReport)).toMatchObject({
      kind: "crash",
      crash: {
        errorMessage: "Boom",
        errorStack: "Error stack",
        componentStack: "Component stack",
      },
      environment: {
        appVersion: "1.12.2",
        platform: "macos",
      },
      latestLog: {
        content: "INFO redacted log line",
      },
    });
  });

  it("submits manual reports to the support endpoint", async () => {
    global.fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ success: true, message: "Report submitted" }), {
        status: 200,
      }),
    );

    await expect(submitManualReport(baseReport)).resolves.toEqual({
      success: true,
      message: "Report submitted",
    });
    expect(fetch).toHaveBeenCalledWith(
      "https://voicetypr.com/api/v1/bug-reports",
      expect.objectContaining({
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(buildManualReportPayload(baseReport)),
      }),
    );
    const [, init] = vi.mocked(fetch).mock.calls[0];
    expect(init?.signal).toBeInstanceOf(AbortSignal);
  });

  it("submits crash reports to the support endpoint", async () => {
    global.fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ success: true, message: "Crash report submitted" }), {
        status: 200,
      }),
    );

    await expect(submitCrashReport(baseCrashReport)).resolves.toEqual({
      success: true,
      message: "Crash report submitted",
    });
    expect(fetch).toHaveBeenCalledWith(
      "https://voicetypr.com/api/v1/bug-reports",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify(buildCrashReportPayload(baseCrashReport)),
      }),
    );
  });

  it("returns a failure result when the network request throws", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    global.fetch = vi.fn().mockRejectedValue(new Error("offline"));

    await expect(submitManualReport(baseReport)).resolves.toEqual({
      success: false,
      message: "Could not connect to Voicetypr Support. Please use Copy Report instead.",
    });
  });

  it("returns a failure result when submit fails", async () => {
    global.fetch = vi
      .fn()
      .mockResolvedValue(
        new Response(
          JSON.stringify({ success: false, message: "Too many reports. Please try again later." }),
          { status: 429 },
        ),
      );

    await expect(submitManualReport(baseReport)).resolves.toEqual({
      success: false,
      message: "Too many reports. Please try again later.",
    });
  });

  it("honors 2xx API envelopes that report failure", async () => {
    global.fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ success: false, message: "Webhook misconfigured." }), {
        status: 200,
      }),
    );

    await expect(submitManualReport(baseReport)).resolves.toEqual({
      success: false,
      message: "Webhook misconfigured.",
    });
  });

  it("returns a failure result when the request times out", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    global.fetch = vi.fn().mockRejectedValue(new DOMException("Timed out", "TimeoutError"));

    await expect(submitManualReport(baseReport)).resolves.toEqual({
      success: false,
      message: "Could not connect to Voicetypr Support. Please use Copy Report instead.",
    });
  });
});
