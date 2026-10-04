import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { appColour } from "@/pill/app-icon";

export function UsageApplicationIcon({ name, iconKey, refreshToken }: { name: string; iconKey?: string | null; refreshToken?: unknown }) {
  const [image, setImage] = useState<{ key: string; src: string } | null>(null);
  const lookup = useRef({ key: iconKey, failures: 0, retryAt: 0, loaded: false, pending: false });
  const mounted = useRef(false);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => {
    if (lookup.current.key !== iconKey) lookup.current = { key: iconKey, failures: 0, retryAt: 0, loaded: false, pending: false };
    const attempt = lookup.current;
    if (!iconKey || attempt.loaded || attempt.pending || Date.now() < attempt.retryAt) return;
    const failed = () => {
      attempt.failures++;
      attempt.retryAt = Date.now() + Math.min(30_000, 1000 * 2 ** Math.min(attempt.failures - 1, 5));
    };
    attempt.pending = true;
    void invoke<string | null>("pill_app_icon", { iconKey }).then(src => {
      attempt.pending = false;
      if (!mounted.current || lookup.current !== attempt) return;
      if (src && /^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(src)) {
        attempt.loaded = true; attempt.failures = 0; setImage({ key: iconKey, src });
      } else failed();
    }).catch(() => {
      attempt.pending = false;
      if (mounted.current && lookup.current === attempt) failed();
    });
  }, [iconKey, refreshToken]);
  return image && image.key === iconKey ? <img aria-hidden src={image.src} className="size-5 shrink-0 object-contain" onError={() => { lookup.current.loaded = false; lookup.current.retryAt = Date.now() + 1000; setImage(null); }} /> : (
    <span aria-hidden className="flex size-5 shrink-0 items-center justify-center rounded-[5px] text-[11px] font-semibold text-white" style={{ background: appColour(name) }}>
      {Array.from(name.trim())[0]?.toUpperCase() ?? "V"}
    </span>
  );
}
