import { invoke } from "@tauri-apps/api/core";
import type { ModelInfo } from "@/types";

export interface ModelStatusResponse {
  models: ModelInfo[];
}

let current: Promise<ModelStatusResponse> | null = null;
let joinable = false;
let queued: Promise<ModelStatusResponse> | null = null;

const run = (): Promise<ModelStatusResponse> => {
  joinable = true;
  // The backend call starts on the next timer tick, so callers reacting to the
  // same event share it, and any later caller gets a trailing request.
  const request: Promise<ModelStatusResponse> = new Promise((resolve, reject) => {
    setTimeout(() => {
      joinable = false;
      invoke<ModelStatusResponse>("get_model_status").then(resolve, reject);
    }, 0);
  }).finally(() => {
    if (current === request) current = null;
  }) as Promise<ModelStatusResponse>;
  current = request;
  return request;
};

/**
 * `get_model_status` with concurrent calls shared. A caller arriving after the
 * in-flight request started waits for one fresh trailing request instead of
 * reusing a result that may predate its triggering event.
 */
export function fetchModelStatus(): Promise<ModelStatusResponse> {
  if (!current) return run();
  if (joinable) return current;
  if (!queued) {
    const wait = current.catch(() => undefined);
    const next = wait.then(() => {
      queued = null;
      return run();
    });
    queued = next;
  }
  return queued;
}
