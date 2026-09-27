import type { BindingResult, SharingModelInfo } from "./types";

export const MIN_SHARING_PORT = 1;
export const MAX_SHARING_PORT = 65535;
export const NO_NETWORK_SENTINEL = "No network connection";

export function isShareableEngine(engine?: string | null): boolean {
  return engine === "whisper" || engine === "parakeet";
}

export function isShareableModel(model: SharingModelInfo): boolean {
  return model.downloaded && model.kind !== "cloud" && isShareableEngine(model.engine);
}

export function parseSharingPort(value: string): number | null {
  const trimmed = value.trim();
  if (!trimmed || !/^\d+$/.test(trimmed)) return null;
  const port = Number(trimmed);
  if (!Number.isInteger(port) || port < MIN_SHARING_PORT || port > MAX_SHARING_PORT) {
    return null;
  }
  return port;
}

export function bindingResultsFromLocalIps(localIps: string[]): BindingResult[] {
  const results: BindingResult[] = [];
  for (const entry of localIps) {
    if (!entry || entry === NO_NETWORK_SENTINEL) continue;
    const match = entry.match(/^(.*?) \((.*?)\)$/);
    results.push({
      ip: match?.[1] ?? entry,
      success: true,
      error: null,
      interface_name: match?.[2],
    });
  }
  return results;
}
