import { calendarWeeks } from "@/components/insights/stats";
import type { UsageDay } from "@/types/usage";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

export function ActivityCalendar({ days }: { days: UsageDay[] }) {
  const weeks = calendarWeeks(days, 43);
  const activeDays = weeks.flat().filter((cell) => cell.active && !cell.future).length;
  return (
    <section
      data-pencil-name="Activity"
      className="rounded-[14px] ring-1 ring-inset ring-border bg-card px-5 pt-4 pb-[18px]"
    >
      <div data-pencil-name="Head" className="flex flex-wrap items-center justify-between gap-2">
        <div data-pencil-name="Title" className="flex items-center gap-2">
          <h2 className="text-sm font-semibold">Activity</h2>
          <span className="text-[12.5px] text-muted-foreground">{activeDays} active days</span>
        </div>
        <div
          data-pencil-name="Legend"
          className="flex items-center gap-[3px] text-[11px] text-muted-foreground"
        >
          <span>Less</span>
          <span className="mx-1 flex gap-[3px]">
            {[0, 1, 2, 3, 4].map((level) => (
              <span
                key={level}
                className="size-2.5 rounded-[2.5px]"
                style={{ background: `var(--activity-${level})` }}
              />
            ))}
          </span>
          <span>More</span>
        </div>
      </div>
      <div className="mt-3 overflow-x-auto pb-0.5">
        <div className="min-w-[676px]">
          <div
            data-pencil-name="Months"
            className="ml-[34px] mb-3 flex h-[14px] gap-[3px] text-[11px] text-muted-foreground"
          >
            {weeks.map((week, i) => {
              const month = week[0].date.getMonth();
              return (
                <div key={week[0].key} className="w-3 shrink-0">
                  {i === 0 || month !== weeks[i - 1][0].date.getMonth()
                    ? week[0].date.toLocaleDateString("en-US", { month: "short" })
                    : ""}
                </div>
              );
            })}
          </div>
          <div data-pencil-name="Body" className="flex gap-2">
            <div
              data-pencil-name="Days"
              aria-hidden
              className="flex w-[26px] shrink-0 flex-col gap-[3px] text-[10px] leading-3 text-muted-foreground"
            >
              {["Mon", "", "Wed", "", "Fri", "", ""].map((label, i) => (
                <span key={i} className="h-3">
                  {label}
                </span>
              ))}
            </div>
            <div
              role="img"
              aria-label={`Activity over 43 weeks, Monday to Sunday: ${activeDays} active days. Today is outlined.`}
              className="flex gap-[3px]"
            >
              {weeks.map((week) => (
                <div data-pencil-name="W" key={week[0].key} className="flex flex-col gap-[3px]">
                  {week.map((cell) => (
                    <Tooltip key={cell.key}>
                      <TooltipTrigger
                        render={
                          <div
                            data-pencil-name="d"
                            className="size-3 rounded-[3px]"
                            style={{
                              background: cell.future
                                ? "transparent"
                                : `var(--activity-${cell.level})`,
                              outline: cell.today ? "1px solid var(--foreground)" : undefined,
                              outlineOffset: -0.5,
                            }}
                          />
                        }
                      />
                      <TooltipContent>
                        {cell.date.toLocaleDateString("en-US", {
                          weekday: "long",
                          month: "long",
                          day: "numeric",
                          year: "numeric",
                        })}{" "}
                        · {cell.words.toLocaleString()} words
                      </TooltipContent>
                    </Tooltip>
                  ))}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
