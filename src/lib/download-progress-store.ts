import { useCallback, useSyncExternalStore } from "react";

export type DownloadProgress = Record<string, number>;

/** Tiny external store so a progress tick re-renders only its subscribers. */
export interface DownloadProgressStore {
  get: () => DownloadProgress;
  update: (updater: (prev: DownloadProgress) => DownloadProgress) => void;
  subscribe: (listener: () => void) => () => void;
}

export function createDownloadProgressStore(): DownloadProgressStore {
  let state: DownloadProgress = {};
  const listeners = new Set<() => void>();
  return {
    get: () => state,
    update: (updater) => {
      const next = updater(state);
      if (next === state) return;
      state = next;
      listeners.forEach((listener) => listener());
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

const noopSubscribe = () => () => undefined;

export function useDownloadProgressStore(
  store: DownloadProgressStore | null,
  select?: undefined,
): DownloadProgress;
export function useDownloadProgressStore<T>(
  store: DownloadProgressStore | null,
  select: (progress: DownloadProgress) => T,
): T;
export function useDownloadProgressStore<T>(
  store: DownloadProgressStore | null,
  select?: (progress: DownloadProgress) => T,
): DownloadProgress | T {
  const getSnapshot = useCallback(() => {
    const progress = store ? store.get() : EMPTY;
    return select ? select(progress) : progress;
  }, [store, select]);
  return useSyncExternalStore(store ? store.subscribe : noopSubscribe, getSnapshot, getSnapshot);
}

const EMPTY: DownloadProgress = {};
