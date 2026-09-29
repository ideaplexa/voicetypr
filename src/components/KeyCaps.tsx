export function KeyCaps({ caps }: { caps: string[] }) {
  if (caps.length === 0) return <span className="text-warn">your recording shortcut</span>;
  return <span aria-label={caps.join(" + ")} className="inline-flex flex-wrap items-center gap-1 align-middle">
    {caps.map((cap, index) => <kbd key={`${cap}-${index}`} className="inline-flex min-h-9 items-center rounded-[9px] border border-border bg-secondary px-2.5 font-sans text-xs font-semibold text-foreground">{cap}</kbd>)}
  </span>;
}
