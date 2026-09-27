import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { emitMockEvent } from "@/test/setup";
import { FeedbackToast } from "./FeedbackToast";

describe("FeedbackToast", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    act(() => {
      vi.runOnlyPendingTimers();
    });
    vi.useRealTimers();
  });

  it("keeps legacy message-based severity inference", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 1,
        message: "Upload failed",
        duration_ms: 2000,
      });
    });

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Upload failed");
    expect(status).toHaveClass("bg-amber-950");
  });

  it("does not auto-clear a persistent warning toast", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 2,
        action: "show",
        message: "Long silence detected",
        duration_ms: 1000,
        variant: "warning",
        persistent: true,
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("Long silence detected");

    act(() => {
      vi.advanceTimersByTime(60_000);
    });

    expect(screen.getByRole("status")).toHaveTextContent("Long silence detected");
  });

  it("clears the current toast when clear id matches", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 3,
        action: "show",
        message: "No audio detected",
        duration_ms: 1000,
        variant: "warning",
        persistent: true,
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("No audio detected");

    act(() => {
      emitMockEvent("toast", {
        id: 3,
        action: "clear",
        message: "",
        duration_ms: 0,
      });
    });

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("does not let a stale clear hide a newer toast", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 4,
        action: "show",
        message: "Long silence detected",
        duration_ms: 1000,
        variant: "warning",
        persistent: true,
      });
      emitMockEvent("toast", {
        id: 5,
        action: "show",
        message: "Copied transcript",
        duration_ms: 5000,
      });
      emitMockEvent("toast", {
        id: 4,
        action: "clear",
        message: "",
        duration_ms: 0,
      });
    });

    expect(screen.getByRole("status")).toHaveTextContent("Copied transcript");
    expect(screen.getByRole("status")).toHaveClass("bg-black");
  });

  it("renders the warning treatment for warning variant payloads", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 6,
        action: "show",
        message: "Check your microphone",
        duration_ms: 1000,
        variant: "warning",
        persistent: true,
      });
    });

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Check your microphone");
    expect(status).toHaveClass("bg-amber-950");
  });

  it("renders suggestion as a second line when present", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 7,
        message: "Microphone access denied",
        duration_ms: 5000,
        suggestion: "Open System Settings > Privacy & Security > Microphone to grant access.",
      });
    });

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Microphone access denied");
    expect(status).toHaveTextContent(
      "Open System Settings > Privacy & Security > Microphone to grant access.",
    );
  });

  it("renders only the message when suggestion is absent", () => {
    render(<FeedbackToast />);

    act(() => {
      emitMockEvent("toast", {
        id: 8,
        message: "Recording failed",
        duration_ms: 3000,
      });
    });

    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("Recording failed");
    // No suggestion line: only the separator span and the message span render.
    const spans = status.querySelectorAll("span");
    expect(spans).toHaveLength(2);
  });
});
