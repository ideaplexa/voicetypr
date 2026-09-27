import type { CSSProperties, ReactNode } from "react";

type PillActivityState = "idle" | "listening" | "transcribing" | "formatting";

interface PillActivityIndicatorProps {
  state: PillActivityState;
  audioLevel?: number;
}

const LISTENING_SAGE = "oklch(0.78 0.1 155)";
const TRANSCRIBING_CYAN = "oklch(0.8 0.1 215)";
const POLISHING_SAGE = "oklch(0.86 0.12 155)";

const INDICATOR_WIDTH = 44;
const INDICATOR_HEIGHT = 16;
const BAR_COUNT = 13;
const CENTER = (BAR_COUNT - 1) / 2;
const BAR_WIDTH = 2;
const BAR_GAP = 1.5;
const REST_SCALE = 0.16;

const ENVELOPE = Array.from({ length: BAR_COUNT }, (_, index) => {
  const distance = Math.abs(index - CENTER) / CENTER;
  return 0.4 + (1 - distance * distance) * 0.6;
});

const SPARK_SIZES = [4, 7, 4] as const;

type ListeningStyle = CSSProperties & {
  "--listening-min": string;
  "--listening-max": string;
};

function ListeningWave({ active, audioLevel }: { active: boolean; audioLevel: number }) {
  return (
    <div
      className="flex items-center justify-center"
      data-visual={active ? "listening" : "idle"}
      style={{ gap: BAR_GAP, height: INDICATOR_HEIGHT, width: INDICATOR_WIDTH }}
    >
      {ENVELOPE.map((envelope, index) => {
        const peak = REST_SCALE + audioLevel * envelope * (1 - REST_SCALE);
        const activeStyle: ListeningStyle = {
          "--listening-min": (peak * 0.45).toFixed(3),
          "--listening-max": peak.toFixed(3),
          animationDelay: `${index * -60}ms`,
        };
        const restingStyle: CSSProperties = {
          opacity: 0.55,
          transform: `scaleY(${(REST_SCALE + envelope * 0.12).toFixed(3)})`,
        };

        return (
          <span
            key={index}
            className={`rounded-full ${active ? "animate-pill-listening" : ""}`}
            style={{
              width: BAR_WIDTH,
              height: INDICATOR_HEIGHT,
              backgroundColor: LISTENING_SAGE,
              transformOrigin: "center",
              ...(active ? activeStyle : restingStyle),
            }}
          />
        );
      })}
    </div>
  );
}

function TranscribingScan() {
  return (
    <div
      className="relative flex shrink-0 items-center justify-center"
      data-visual="transcribing"
      style={{ height: INDICATOR_HEIGHT, width: INDICATOR_WIDTH }}
    >
      <span
        className="h-px w-9 rounded-full opacity-35"
        style={{ backgroundColor: TRANSCRIBING_CYAN }}
      />
      <span
        className="animate-pill-scan absolute left-1 size-1.5 rounded-full"
        style={{ backgroundColor: TRANSCRIBING_CYAN }}
      />
    </div>
  );
}

function PolishingSparkles() {
  return (
    <div
      className="flex shrink-0 items-center justify-center gap-1.5"
      data-visual="formatting"
      style={{ height: INDICATOR_HEIGHT, width: INDICATOR_WIDTH }}
    >
      {SPARK_SIZES.map((size, index) => (
        <span
          key={`${size}-${index}`}
          className="animate-pill-spark block rounded-[1px]"
          style={{
            animationDelay: `${index * -320}ms`,
            backgroundColor: POLISHING_SAGE,
            height: size,
            width: size,
          }}
        />
      ))}
    </div>
  );
}

export function PillActivityIndicator({ state, audioLevel = 0 }: PillActivityIndicatorProps) {
  const level = Math.max(0, Math.min(1, audioLevel));
  let visual: ReactNode;

  if (state === "transcribing") {
    visual = <TranscribingScan />;
  } else if (state === "formatting") {
    visual = <PolishingSparkles />;
  } else {
    visual = <ListeningWave active={state === "listening"} audioLevel={level} />;
  }

  return (
    <div
      aria-hidden="true"
      className="flex shrink-0 items-center justify-center"
      data-state={state}
      data-testid="pill-activity"
    >
      {visual}
    </div>
  );
}
