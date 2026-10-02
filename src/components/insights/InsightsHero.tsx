import { AudioLines, Flame, Gauge, Timer } from "lucide-react";
import { savedTime, type computeUsageOverview } from "@/components/insights/stats";

export function InsightsHero({ overview }: { overview: ReturnType<typeof computeUsageOverview> }) {
  const metrics = [
    {
      name: "Streak",
      icon: Flame,
      value: overview.current.toLocaleString(),
      unit: "days",
      note: `Best: ${overview.best} days`,
    },
    ...(overview.pace !== null
      ? [
          {
            name: "Pace",
            icon: Gauge,
            value: Math.round(overview.pace).toLocaleString(),
            unit: "wpm",
            note: `${(overview.pace / 40).toFixed(1)}× your typing`,
          },
        ]
      : []),
    {
      name: "Dictations",
      icon: AudioLines,
      value: overview.dictations.toLocaleString(),
      unit: "",
      note: `${overview.average} words on average`,
    },
  ];
  return (
    <section
      data-pencil-name="Hero"
      className="flex flex-wrap items-center gap-5 rounded-[14px] ring-1 ring-inset ring-border bg-card px-[22px] py-5"
    >
      <div data-pencil-name="Words" className="min-w-[180px] flex-1">
        <p data-pencil-name="Label" className="text-[12.5px] font-medium text-muted-foreground">
          Words dictated
        </p>
        <p
          data-pencil-name="Value"
          className="font-mono text-[36px] leading-[normal] font-semibold tracking-[-1.2px]"
        >
          {overview.words.toLocaleString()}
        </p>
        <p
          data-pencil-name="Saved"
          className="mt-0.5 flex items-center gap-1.5 text-[12.5px] font-medium text-sage"
        >
          <Timer className="size-3.5" />
          {savedTime(overview.savedMinutes)} saved vs typing
        </p>
      </div>
      {metrics.map(({ name, icon: Icon, value, unit, note }) => (
        <div
          data-pencil-name={name}
          key={name}
          className="flex min-h-16 w-[136px] flex-col justify-center gap-1 border-l border-border pl-5"
        >
          <p
            data-pencil-name="Top"
            className="flex items-center gap-[5px] text-[12.5px] font-medium text-muted-foreground"
          >
            <Icon className="size-3.5" />
            {name}
          </p>
          <p data-pencil-name="Value" className="flex items-baseline gap-1">
            <strong className="font-mono text-[22px] leading-[normal] font-semibold tracking-[-0.6px]">
              {value}
            </strong>
            <span className="text-[12.5px] text-muted-foreground">{unit}</span>
          </p>
          <p data-pencil-name="Note" className="whitespace-nowrap text-[11.5px] text-text-3">
            {note}
          </p>
        </div>
      ))}
    </section>
  );
}
