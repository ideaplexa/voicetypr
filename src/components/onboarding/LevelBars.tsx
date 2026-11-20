import { cn } from "@/lib/utils";
import { useState } from "react";

/** A live level meter as the frames draw it: recent levels as bars, newest last, quiet slots greyed. */
export function LevelBars({ level, size }: { level: number; size: "sm" | "lg" }) {
  const count = size === "sm" ? 16 : 24;
  const [seen, setSeen] = useState<{ level: number; history: number[] }>({ level, history: [level] });
  if (seen.level !== level) setSeen({ level, history: [...seen.history, level].slice(-count) });
  const history = seen.history;
  const slots = Array.from({ length: count }, (_, index) => history[index - (count - history.length)] ?? 0);
  const [min, max] = size === "sm" ? [4, 22] : [6, 60];
  return (
    <span
      role="meter"
      aria-label="Microphone input level"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(level * 100)}
      className={cn("flex items-center", size === "sm" ? "h-[22px] gap-[3px]" : "h-16 gap-[5px]")}
    >
      {slots.map((value, index) => (
        <span
          key={index}
          className={cn(value > 0.02 ? "bg-sage" : "bg-border", size === "sm" ? "w-1 rounded-[2px]" : "w-2 rounded-[4px]")}
          style={{ height: value > 0.02 ? Math.round(min + (max - min) * value) : min }}
        />
      ))}
    </span>
  );
}
