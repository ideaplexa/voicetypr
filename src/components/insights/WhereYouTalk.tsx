import type { UsageApp } from "@/types/usage";

export function WhereYouTalk({ apps, loading }: { apps: UsageApp[]; loading: boolean }) {
  const total = apps.reduce((sum, app) => sum + app.words, 0);
  return (
    <section
      data-pencil-name="Where you talk"
      className="min-w-0 rounded-[14px] ring-1 ring-inset ring-border bg-card px-5 py-4"
    >
      <div data-pencil-name="Head" className="mb-3 flex items-center justify-between">
        <h2 className="text-sm font-semibold">Where you talk</h2>
        <span className="text-xs text-text-3">by words</span>
      </div>
      {loading ? (
        <p role="status" className="text-xs text-muted-foreground">
          Loading apps…
        </p>
      ) : total === 0 ? (
        <p className="text-[13px] text-muted-foreground">
          App stats appear when you dictate into an app.
        </p>
      ) : (
        <ul className="flex flex-col gap-[9px]">
          {[...apps]
            .sort((a, b) => b.words - a.words)
            .slice(0, 5)
            .map((app, i) => (
              <li key={app.name} data-pencil-name="App" className="flex items-center gap-[10px]">
                <span
                  aria-hidden
                  className="flex size-5 shrink-0 items-center justify-center rounded-[5px] bg-sage-bg text-[11px] font-semibold text-sage"
                  style={{
                    background: `color-mix(in srgb, var(--sage) ${20 + i * 10}%, var(--card))`,
                  }}
                >
                  {app.name.slice(0, 1).toUpperCase()}
                </span>
                <span className="w-[64px] shrink-0 truncate text-[12.5px]" title={app.name}>
                  {app.name}
                </span>
                <div
                  data-pencil-name="Track"
                  className="h-1.5 min-w-3 flex-1 overflow-hidden rounded bg-muted"
                >
                  <div
                    data-pencil-name="Bar"
                    className="h-full rounded bg-sage"
                    style={{ width: `${(app.words / total) * 100}%`, opacity: i === 0 ? 1 : 0.45 }}
                  />
                </div>
                <span className="w-[30px] text-right font-mono text-[11.5px] text-muted-foreground">
                  {Math.round((app.words / total) * 100)}%
                </span>
              </li>
            ))}
        </ul>
      )}
    </section>
  );
}
