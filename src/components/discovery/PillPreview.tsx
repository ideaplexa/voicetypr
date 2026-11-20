import { useEffect, useRef } from "react";
import { Glyph } from "@/pill/glyph";
import { appColour } from "@/pill/app-icon";
import { BAR_SECONDS } from "@/pill/wave";
import { waveZone } from "@/pill/zones";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";

export type PillPreviewState = "listening" | "live" | "transcribing" | "polishing" | "pasted";

const SPEECH = [0, 0, 0, 0, 0.22, 0.48, 0.7, 0.86, 0.95, 0.8, 0.9, 1, 0.85, 0.72, 0.95, 0.62, 0.34, 0.8, 0.9, 0.55, 0.86];
const SILENCE = SPEECH.map(() => 0);
const SAGE = "#8FD1A8";
const DIM = "rgba(255,255,255,.55)";

/**
 * The recording pill, small and still, for illustrations. The glyph (bars, spinner, check) is drawn by the
 * pill's own Glyph and WaveBuffer on a canvas of their own; the live pill is never touched. Decorative only.
 */
export function PillPreview({ state = "listening", silent = false, app = "Notes", words = ["Let’s move the", "meeting"], scale = 0.5, className }: {
  state?: PillPreviewState;
  silent?: boolean;
  app?: string;
  words?: [string, string];
  scale?: number;
  className?: string;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const live = state === "live";
  const recording = state === "listening" || live;
  const width = live ? 300 : 236;
  const height = live ? 64 : 34;
  const row = height - 2 - 16;
  const timer = recording ? (silent ? "0:00" : "0:03") : "0:04";
  const waveWidth = waveZone(width - 72, timer).width;
  const glyphLeft = recording ? 38 : state === "pasted" ? 55 : 34;

  useEffect(() => {
    if (!canvas.current) return;
    const glyph = new Glyph(canvas.current);
    glyph.target(state);
    let seconds = 0;
    for (const level of recording ? (silent ? SILENCE : SPEECH) : []) {
      glyph.wave.sample(level);
      seconds += BAR_SECONDS;
      glyph.wave.advance(seconds, BAR_SECONDS);
    }
    glyph.step(1, true);
    glyph.step(1, true);
    glyph.draw(seconds + BAR_SECONDS, waveWidth, !recording);
  }, [state, silent, recording, waveWidth]);

  const radius = Math.min(height / 2, 20);
  const r = (height - 2) / 2;
  return (
    <span aria-hidden className={cn("relative block shrink-0", className)} style={{ width: width * scale, height: height * scale }}>
      <span
        className="absolute top-0 left-0 origin-top-left overflow-clip font-sans text-[12.5px] leading-[normal] whitespace-nowrap select-none"
        style={{ width, height, transform: `scale(${scale})`, borderRadius: radius, background: "#121316", border: "1px solid rgba(0,0,0,.45)", boxShadow: "inset 0 0 0 1px rgba(255,255,255,.16), 0 12px 34px rgba(0,0,0,.3)", color: "rgba(255,255,255,.92)" }}
      >
        {live ? (
          <span className="absolute inset-x-0 top-0 h-[30px] overflow-clip px-[13px] pt-[9px]">
            {words[0]} <i className="pr-0.5" style={{ color: "rgba(255,255,255,.52)" }}>{words[1]}</i>
          </span>
        ) : null}
        <span className="absolute flex size-[18px] items-center justify-center text-[11px] font-semibold text-white" style={{ left: 12, top: row - 9, borderRadius: isMacOS ? 5 : 4, background: appColour(app) }}>
          {Array.from(app.trim())[0]?.toUpperCase() ?? "V"}
        </span>
        <canvas ref={canvas} className="absolute" style={{ left: glyphLeft, top: row - 12, width: 260, height: 24 }} />
        {recording ? (
          <>
            <span className="absolute font-mono text-[12px] tabular-nums" style={{ left: width - 98, top: row - 8, color: DIM }}>{timer}</span>
            <span className="absolute flex size-6 items-center justify-center rounded-full" style={{ left: width - 29, top: row - 12, background: "rgba(143,209,168,.18)" }}>
              <i className="block size-2 rounded-[1.5px]" style={{ background: SAGE }} />
            </span>
          </>
        ) : null}
        {state === "transcribing" || state === "polishing" ? (
          <>
            <span className="absolute" style={{ left: 58, top: row - 8 }}>{state === "polishing" ? "Polishing" : "Transcribing"}</span>
            <span className="absolute font-mono text-[12px] tabular-nums" style={{ right: 12, top: row - 8, color: DIM }}>{timer}</span>
          </>
        ) : null}
        {state === "pasted" ? (
          <>
            <span className="absolute" style={{ left: 81, top: row - 8 }}>Pasted <span style={{ color: DIM }}>· 12 words</span></span>
            <svg className="absolute -top-px -left-px overflow-visible" width={width} height={height}>
              <path d={`M ${width / 2} 1 H ${width - 1 - r} A ${r} ${r} 0 0 1 ${width - 1 - r} ${height - 1} H ${1 + r} A ${r} ${r} 0 0 1 ${1 + r} 1 Z`} pathLength={1} fill="none" stroke={SAGE} strokeWidth={2} strokeDasharray="1 1" strokeDashoffset={0.45} />
            </svg>
          </>
        ) : null}
      </span>
    </span>
  );
}
