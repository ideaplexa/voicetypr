import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { PillActivityIndicator } from "./PillActivityIndicator";

function centerListeningBar(container: HTMLElement): HTMLElement {
  const bars = container.querySelectorAll<HTMLElement>('[data-visual="listening"] > span');
  return bars[Math.floor(bars.length / 2)];
}

describe("PillActivityIndicator", () => {
  it("uses a live waveform whose height follows the audio level while listening", () => {
    const quiet = centerListeningBar(
      render(<PillActivityIndicator state="listening" audioLevel={0} />).container,
    );
    const loud = centerListeningBar(
      render(<PillActivityIndicator state="listening" audioLevel={1} />).container,
    );

    expect(loud.className).toContain("animate-pill-listening");
    const quietPeak = Number(quiet.style.getPropertyValue("--listening-max"));
    const loudPeak = Number(loud.style.getPropertyValue("--listening-max"));
    expect(quietPeak).toBeLessThan(0.25);
    expect(loudPeak).toBeGreaterThan(0.9);
    expect(loudPeak).toBeGreaterThan(quietPeak);
  });

  it("exposes every backend phase on the stable activity container", () => {
    for (const state of ["idle", "listening", "transcribing", "formatting"] as const) {
      const { container, unmount } = render(<PillActivityIndicator state={state} />);
      expect(container.querySelector('[data-testid="pill-activity"]')).toHaveAttribute(
        "data-state",
        state,
      );
      unmount();
    }
  });

  it("uses unmistakably different visuals for transcribing and polishing", () => {
    const transcribing = render(<PillActivityIndicator state="transcribing" />).container;
    const formatting = render(<PillActivityIndicator state="formatting" />).container;

    expect(transcribing.querySelector('[data-visual="transcribing"]')).toBeInTheDocument();
    expect(transcribing.querySelector(".animate-pill-scan")).toBeInTheDocument();
    expect(transcribing.querySelector('[data-visual="formatting"]')).not.toBeInTheDocument();

    expect(formatting.querySelector('[data-visual="formatting"]')).toBeInTheDocument();
    expect(formatting.querySelectorAll(".animate-pill-spark")).toHaveLength(3);
    expect(formatting.querySelector('[data-visual="transcribing"]')).not.toBeInTheDocument();
  });
});
