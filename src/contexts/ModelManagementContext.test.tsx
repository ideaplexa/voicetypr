import { act, render } from "@testing-library/react";
import { useEffect } from "react";
import { describe, expect, it, vi } from "vitest";


const counters = vi.hoisted(() => ({ hookRenders: 0 }));
const store = vi.hoisted(() => {
  // Plain stand-in created before the hoisted mock factory runs.
  return { current: null as null | import("@/lib/download-progress-store").DownloadProgressStore };
});
vi.mock("@/hooks/useModelManagement", async () => {
  const { useDownloadProgressStore, createDownloadProgressStore } = await import("@/lib/download-progress-store");
  const stable = {
    models: {},
    modelOrder: [] as string[],
    verifyingModels: new Set<string>(),
    downloadPhases: {},
    downloadErrors: {},
    isLoading: false,
    loadModels: vi.fn(),
    downloadModel: vi.fn(),
    cancelDownload: vi.fn(),
    deleteModel: vi.fn(),
    repairModel: vi.fn(),
    sortedModels: [] as never[],
    progressStore: (store.current ??= createDownloadProgressStore()),
  };
  return {
    useModelManagement: () => {
      counters.hookRenders += 1;
      // Mirrors the real hook: a fresh object every render.
      return { ...stable, downloadProgress: useDownloadProgressStore(null) };
    },
  };
});

import {
  ModelManagementProvider,
  useDownloadProgress,
  useModelManagementContext,
} from "./ModelManagementContext";

describe("ModelManagementContext render isolation", () => {
  it("a progress tick re-renders only the progress consumer", () => {
    const plain = { count: 0 };
    const progress = { count: 0 };
    function Plain() {
      useModelManagementContext();
      useEffect(() => void (plain.count += 1));
      return null;
    }
    function Progress() {
      const value = useDownloadProgress((p) => p["base.en"]);
      useEffect(() => void (progress.count += 1));
      return <span>{value ?? "none"}</span>;
    }
    render(
      <ModelManagementProvider>
        <Plain />
        <Progress />
      </ModelManagementProvider>,
    );
    const [plainBase, progressBase, hookBase] = [plain.count, progress.count, counters.hookRenders];
    for (const value of [10, 20, 30]) {
      act(() => store.current!.update((prev) => ({ ...prev, "base.en": value })));
    }
    expect(plain.count).toBe(plainBase);
    expect(counters.hookRenders).toBe(hookBase);
    expect(progress.count).toBe(progressBase + 3);
  });
});
