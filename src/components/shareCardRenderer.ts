import { createLogger } from "@/lib/logger";
import { WORDS_PER_PAGE } from "./shareStats";

const log = createLogger("share-stats");

const LOGO_SRC = `${import.meta.env.BASE_URL}logo.png`;

export interface ShareCardStats {
  totalTranscriptions: number;
  totalWords: number;
  timeSavedDisplay: string;
}

export function getSharePlays(
  totalWords: number,
  totalTranscriptions: number,
  timeSavedDisplay: string,
): Array<{ value: string; play: string }> {
  const timeSaved = timeSavedDisplay === "0m" ? "0m" : timeSavedDisplay;
  const plays = [
    {
      value: totalWords.toLocaleString(),
      play: totalWords === 1 ? "word I spoke" : "words I spoke",
    },
  ];
  if (totalWords >= 250) {
    const pages = Math.max(1, Math.round(totalWords / WORDS_PER_PAGE));
    plays.push({
      value: pages.toLocaleString(),
      play: pages === 1 ? "page I didn’t type" : "pages I didn’t type",
    });
  }
  plays.push({
    value: timeSaved,
    play: "my fingers got back",
  });
  plays.push({
    value: totalTranscriptions.toLocaleString(),
    play:
      totalTranscriptions === 1 ? "time I skipped the keyboard" : "times I skipped the keyboard",
  });
  return plays;
}

async function loadShareCardLogo(): Promise<HTMLImageElement | null> {
  return new Promise<HTMLImageElement | null>((resolve) => {
    const image = new Image();
    image.decoding = "async";
    image.onload = () => resolve(image);
    image.onerror = () => resolve(null);
    image.src = LOGO_SRC;
  });
}

export async function drawShareCard(
  canvas: HTMLCanvasElement,
  stats: ShareCardStats,
  isCancelled: () => boolean,
): Promise<string | null> {
  const context = canvas.getContext("2d");
  if (!context) {
    log.error("Could not create the share card canvas");
    return null;
  }

  const logo = await loadShareCardLogo();
  if (isCancelled()) return null;

  const logicalWidth = 1200;
  const logicalHeight = 800;
  const exportScale = 2;
  canvas.width = logicalWidth * exportScale;
  canvas.height = logicalHeight * exportScale;
  context.resetTransform();
  context.scale(exportScale, exportScale);
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = "high";

  const fontFamily = "'Geist Variable', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif";
  const cream = "#fffaf2";
  const mint = "#8ed6a3";
  const teal = "#4fc9c7";
  const ink = "#0f1711";
  const plays = getSharePlays(stats.totalWords, stats.totalTranscriptions, stats.timeSavedDisplay);
  const centerX = logicalWidth / 2;

  const background = context.createLinearGradient(0, 0, logicalWidth, logicalHeight);
  background.addColorStop(0, "#17181c");
  background.addColorStop(1, "#101113");
  context.fillStyle = background;
  context.fillRect(0, 0, logicalWidth, logicalHeight);

  const glow = context.createRadialGradient(centerX, 200, 30, centerX, 200, 520);
  glow.addColorStop(0, "rgba(79, 201, 199, 0.12)");
  glow.addColorStop(0.5, "rgba(142, 214, 163, 0.06)");
  glow.addColorStop(1, "rgba(79, 201, 199, 0)");
  context.fillStyle = glow;
  context.fillRect(0, 0, logicalWidth, logicalHeight);

  const accent = (x0: number, x1: number) => {
    const gradient = context.createLinearGradient(x0, 0, x1, 0);
    gradient.addColorStop(0, mint);
    gradient.addColorStop(1, teal);
    return gradient;
  };

  if (logo) {
    context.save();
    [
      { radius: 78, alpha: 0.16 },
      { radius: 102, alpha: 0.1 },
      { radius: 126, alpha: 0.05 },
    ].forEach(({ radius, alpha }) => {
      context.strokeStyle = `rgba(142, 214, 163, ${alpha})`;
      context.lineWidth = 1.5;
      context.beginPath();
      context.arc(centerX, 112, radius, 0, Math.PI * 2);
      context.stroke();
    });
    context.restore();
    context.drawImage(logo, centerX - 48, 64, 96, 96);
  }

  context.textAlign = "center";
  context.fillStyle = accent(centerX - 140, centerX + 140);
  context.font = `560 26px ${fontFamily}`;
  context.fillText("type with your voice.", centerX, 228);

  const rowTop = 322;
  const rowHeight = 88;
  plays.forEach((item, index) => {
    const y = rowTop + index * rowHeight;
    context.font = `720 50px ${fontFamily}`;
    const numberWidth = context.measureText(item.value).width;
    context.font = `520 32px ${fontFamily}`;
    const playWidth = context.measureText(item.play).width;
    const rowWidth = numberWidth + 30 + playWidth;
    const numberX = centerX - rowWidth / 2;
    context.textAlign = "left";
    context.fillStyle = cream;
    context.font = `720 50px ${fontFamily}`;
    context.fillText(item.value, numberX, y);
    context.fillStyle = accent(numberX, numberX + rowWidth);
    context.font = `520 32px ${fontFamily}`;
    context.fillText(item.play, numberX + numberWidth + 30, y);
  });

  const ctaText = "Try Voicetypr free";
  context.font = `640 28px ${fontFamily}`;
  const ctaWidth = context.measureText(ctaText).width + 88;
  const ctaX = centerX - ctaWidth / 2;
  const ctaY = 636;
  context.save();
  context.shadowColor = "rgba(79, 201, 199, 0.35)";
  context.shadowBlur = 28;
  context.shadowOffsetY = 6;
  context.fillStyle = accent(ctaX, ctaX + ctaWidth);
  context.beginPath();
  context.roundRect(ctaX, ctaY, ctaWidth, 66, 33);
  context.fill();
  context.restore();
  context.textAlign = "center";
  context.textBaseline = "middle";
  context.fillStyle = ink;
  context.font = `640 28px ${fontFamily}`;
  context.fillText(ctaText, centerX, ctaY + 34);
  context.textBaseline = "alphabetic";

  context.fillStyle = "#b8b0a6";
  context.font = `560 20px ${fontFamily}`;
  context.fillText("voicetypr.com · no card required", centerX, 738);

  if (isCancelled()) return null;
  try {
    return canvas.toDataURL("image/png");
  } catch (error) {
    log.error("Could not encode the share card", error);
    return null;
  }
}
