import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

export function useMicrophoneTest(deviceName: string | undefined) {
  const [visible, setVisible] = useState(true);
  const [level, setLevel] = useState(0);
  const [status, setStatus] = useState<"opening" | "listening" | "heard" | "quiet" | "error" | "paused">("opening");
  useEffect(() => {
    let disposed = false;
    const listener = listen<boolean>("main-window-visibility-changed", ({ payload }) => {
      if (!disposed) setVisible(payload);
    });
    return () => { disposed = true; void listener.then((unlisten) => unlisten()).catch(() => undefined); };
  }, []);
  useEffect(() => {
    if (!visible) return;
    let disposed = false;
    let sessionId: string | undefined;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    let heard = false;
    setStatus("opening");
    setLevel(0);
    const listener = listen<{ session_id: string; level: number; ended?: boolean; failed?: boolean }>("microphone-test-level", ({ payload }) => {
      if (disposed || payload.session_id !== sessionId) return;
      if (payload.ended) {
        clearTimeout(timeout);
        setLevel(0);
        setStatus(payload.failed ? "error" : "paused");
        return;
      }
      setLevel(Math.max(0, Math.min(1, payload.level * 8)));
      if (payload.level > 0.008) {
        heard = true;
        setStatus("heard");
      }
    });
    void listener.then(async () => {
      if (disposed) return;
      const id = await invoke<string>("start_microphone_test", { deviceName: deviceName || null });
      sessionId = id;
      if (disposed) {
        await invoke("stop_microphone_test", { sessionId: id });
        return;
      }
      setStatus("listening");
      timeout = setTimeout(() => { if (!disposed && !heard) setStatus("quiet"); }, 4000);
    }).catch(() => { if (!disposed) setStatus("error"); });
    return () => {
      disposed = true;
      clearTimeout(timeout);
      void listener.then((unlisten) => unlisten()).catch(() => undefined);
      if (sessionId) void invoke("stop_microphone_test", { sessionId }).catch(() => undefined);
    };
  }, [deviceName, visible]);
  return { level: visible ? level : 0, status, visible };
}
