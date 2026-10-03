import {
  computeUsageOverview,
  calendarWeeks,
  parseLocalDay,
  periodLabels,
  type UsagePeriod,
} from "@/components/insights/stats";
import type { UsageDay, UsageStats } from "@/types/usage";

export interface ShareCardStats {
  totalWords: number;
  timeSavedDisplay: string;
  streak: number;
  pace: number | null;
  range: string;
  days: UsageDay[];
}
export function toShareCardStats(stats: UsageStats, period: UsagePeriod = "all"): ShareCardStats {
  const overview = computeUsageOverview(stats, period);
  const since = stats.first_use
    ? parseLocalDay(stats.first_use).toLocaleDateString("en-US", { month: "long", year: "numeric" })
    : null;
  return {
    totalWords: overview.words,
    timeSavedDisplay:
      overview.savedMinutes >= 60
        ? `${Math.floor(overview.savedMinutes / 60)} h`
        : `${overview.savedMinutes} m`,
    streak: overview.current,
    pace: overview.pace,
    range: `${periodLabels[period]}${period === "all" && since ? ` · since ${since}` : ""}`,
    days: stats.days,
  };
}

/** Numbers and dates only. The canvas has exactly the exported 1200×630 dimensions. */
export async function drawShareCard(
  canvas: HTMLCanvasElement,
  stats: ShareCardStats,
  isCancelled: () => boolean,
): Promise<string | null> {
  await Promise.all([
    document.fonts.load('600 58px "Geist Mono Variable"'),
    document.fonts.load('600 14px "Geist Variable"'),
    document.fonts.ready,
  ]);
  if (isCancelled()) return null;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  canvas.width = 1200;
  canvas.height = 630;
  ctx.resetTransform();
  ctx.scale(2, 2);
  ctx.fillStyle = "#121316";
  ctx.beginPath();
  ctx.roundRect(0, 0, 600, 315, 16);
  ctx.fill();
  const sans = '"Geist Variable", sans-serif';
  const mono = '"Geist Mono Variable", monospace';
  const text = (
    value: string,
    x: number,
    y: number,
    size: number,
    color: string,
    family = sans,
    weight = 400,
  ) => {
    ctx.fillStyle = color;
    ctx.font = `${weight} ${size}px ${family}`;
    ctx.fillText(value, x, y);
  };
  ctx.fillStyle = "#8FD1A8";
  ctx.beginPath();
  ctx.roundRect(30, 26, 22, 22, 6);
  ctx.fill();
  // The real Brandmark.tsx voice V, in its original SVG coordinate system.
  ctx.save();
  ctx.translate(33, 29);
  ctx.scale(16 / 312, 16 / 320);
  ctx.translate(-100, -96);
  ctx.strokeStyle = "#121316";
  ctx.fillStyle = "#121316";
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const paths: [string, number][] = [
    ["M143 197 C166 288 201 350 240 393", 36],
    ["M369 197 C346 288 311 350 272 393", 36],
    ["M256 163 L256 338", 23],
    ["M202 226 L202 257", 15],
    ["M226 207 L226 276", 15],
    ["M286 207 L286 276", 15],
    ["M310 226 L310 257", 15],
  ];
  for (const [path, width] of paths) {
    ctx.lineWidth = width;
    ctx.stroke(new Path2D(path));
  }
  ctx.beginPath();
  ctx.arc(256, 124, 18, 0, Math.PI * 2);
  ctx.fill();
  ctx.restore();
  text("Voicetypr", 60, 42, 14, "#FFFFFFEB", sans, 600);
  ctx.textAlign = "right";
  text(stats.range, 570, 41, 12, "#FFFFFF8C");
  ctx.textAlign = "left";
  ctx.letterSpacing = "-2.4px";
  // Shrink only for large totals that would otherwise collide with the calendar.
  ctx.font = `600 58px ${mono}`;
  const size = Math.min(
    58,
    (58 * 360) / Math.max(360, ctx.measureText(stats.totalWords.toLocaleString()).width),
  );
  text(stats.totalWords.toLocaleString(), 30, 158, size, "#FFFFFF", mono, 600);
  ctx.letterSpacing = "0px";
  text("words spoken, not typed", 30, 193, 16, "#8FD1A8", sans, 500);
  const colors = ["#FFFFFF12", "#8FD1A833", "#8FD1A866", "#8FD1A8A6", "#8FD1A8"];
  calendarWeeks(stats.days, 14).forEach((week, col) =>
    week.forEach((cell, row) => {
      if (cell.future) return;
      ctx.fillStyle = colors[cell.level];
      ctx.beginPath();
      ctx.roundRect(405 + col * 12, 109 + row * 12, 9, 9, 2.5);
      ctx.fill();
    }),
  );
  let x = 30;
  const metrics = [
    { value: stats.timeSavedDisplay, label: "saved vs typing" },
    { value: `${stats.streak} days`, label: "streak" },
    ...(stats.pace === null
      ? []
      : [{ value: `${Math.round(stats.pace)} wpm`, label: "speaking pace" }]),
  ];
  for (const metric of metrics) {
    text(metric.value, x, 268, 18, "#FFFFFF", mono, 600);
    const width = ctx.measureText(metric.value).width;
    text(metric.label, x, 286, 11.5, "#FFFFFF8C");
    x += Math.max(width, ctx.measureText(metric.label).width) + 26;
  }
  ctx.textAlign = "right";
  text("voicetypr.com", 570, 285, 12, "#FFFFFF8C", mono);
  ctx.textAlign = "left";
  if (isCancelled()) return null;
  return canvas.toDataURL("image/png");
}
