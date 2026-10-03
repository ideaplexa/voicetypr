import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { appColour } from "@/pill/app-icon";

export function UsageApplicationIcon({ name, iconKey }: { name: string; iconKey?: string | null }) {
  const [image, setImage] = useState<{ key: string; src: string } | null>(null);
  useEffect(() => {
    if (!iconKey) return;
    let active = true;
    void invoke<string | null>("pill_app_icon", { iconKey }).then(src => {
      if (active && src && /^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(src)) setImage({ key: iconKey, src });
    }).catch(() => {});
    return () => { active = false; };
  }, [iconKey]);
  return image && image.key === iconKey ? <img aria-hidden src={image.src} className="size-5 shrink-0 object-contain" onError={() => setImage(null)} /> : (
    <span aria-hidden className="flex size-5 shrink-0 items-center justify-center rounded-[5px] text-[11px] font-semibold text-white" style={{ background: appColour(name) }}>
      {Array.from(name.trim())[0]?.toUpperCase() ?? "V"}
    </span>
  );
}
