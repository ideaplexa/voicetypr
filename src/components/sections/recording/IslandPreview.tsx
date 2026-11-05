import { useEffect, useRef } from "react";
import { createIsland } from "@/pill/renderer";
import { emptyContext } from "@/pill/contracts";
import type { PillIndicatorPosition } from "@/types";
import { isMacOS } from "@/lib/platform";
import pillCss from "@/pill.css?inline";

export function IslandPreview({ position }: { position: PillIndicatorPosition }) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!host.current) return;
    const shadow = host.current.shadowRoot ?? host.current.attachShadow({ mode: "open" });
    const style = document.createElement("style");
    style.textContent = `${pillCss}\n.pill-root { position:absolute }`;
    const root = document.createElement("div");
    root.inert = true;
    shadow.replaceChildren(style, root);
    const noop = async () => {};
    const island = createIsland(root, {
      staticPreview: true, mac: isMacOS, icon: async () => null, hitRegions: () => {},
      actions: { toggle: noop, stop: noop, cancel: noop, dismiss: noop },
      setTimeout: (() => 0) as unknown as typeof window.setTimeout, clearTimeout: () => {},
    });
    island.settings({ pill_indicator_mode: "always" });
    island.geometry({ anchor: position, anchorX: position.endsWith("-left") ? 12 : position.endsWith("-right") ? 288 : 150, anchorY: position.startsWith("top-") ? 16 : 104 });
    const context = emptyContext();
    context.generation = 1;
    context.app.name = "App";
    island.context(context);
    island.start();
    island.machine.startedAt = performance.now() - 7000;
    island.level({ generation: 1, level: 0.5 });
    island.renderStatic();
    return () => { island.destroy(); shadow.replaceChildren(); };
  }, [position]);
  return <div ref={host} role="img" aria-label={`Listening island at ${position.replace(/-/g, " ")}`} className="relative h-[120px] w-[300px] max-w-full overflow-hidden rounded-[10px] bg-muted" />;
}
