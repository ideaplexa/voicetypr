export function KeyCaps({
  caps,
  size = "sm",
}: {
  caps: string[];
  size?: "lg" | "sm" | "onboarding";
}) {
  if (caps.length === 0) return <span className="text-warn">your recording shortcut</span>;
  return (
    <span
      aria-label={caps.join(" + ")}
      className="inline-flex flex-wrap items-center gap-1 align-middle"
    >
      {caps.map((cap, index) => (
        <kbd
          key={`${cap}-${index}`}
          className={
            size === "onboarding"
              ? "inline-flex items-center rounded-[7px] border border-border bg-card px-[10px] py-[5px] font-sans text-[15px] font-semibold leading-[normal] text-foreground"
              : size === "lg"
                ? "inline-flex items-center rounded-[9px] border border-border bg-secondary px-3 py-1 font-sans text-[24px] leading-[normal] font-semibold text-foreground"
                : "inline-flex min-h-9 items-center rounded-[9px] border border-border bg-secondary px-2.5 font-sans text-xs font-semibold text-foreground"
          }
        >
          {cap}
        </kbd>
      ))}
    </span>
  );
}
