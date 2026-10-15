import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TranscriptionHistory } from "@/types";
import { RecentRecordingsList } from "./RecentRecordingsList";

const iconRenders = vi.hoisted(() => ({ byApp: new Map<string, number>() }));

vi.mock("./RecentRecordingApplicationIcon", () => ({
  RecentRecordingApplicationIcon: ({ appName }: { appName: string }) => {
    iconRenders.byApp.set(appName, (iconRenders.byApp.get(appName) ?? 0) + 1);
    return null;
  },
}));

function makeItem(id: string): TranscriptionHistory {
  return {
    id,
    text: `text ${id}`,
    model: "base",
    timestamp: new Date("2026-10-01T09:05:00"),
    writing: { context_hint: { app_name: `App-${id}`, process_path: `/apps/${id}` } },
  } as unknown as TranscriptionHistory;
}

const items = ["a", "b", "c"].map(makeItem);

function renderList(selectedId: string | null, onSelect = vi.fn()) {
  const props = {
    historyLength: items.length,
    filteredHistory: items,
    groupedHistory: { Today: items },
    visibleCount: 60,
    onLoadMore: vi.fn(),
    isLoading: false,
    loadError: null,
    hotkey: "Alt+Space",
    readinessStatus: { ready: true, label: "Ready" } as never,
    onReadinessAction: vi.fn(),
  };
  const view = render(
    <RecentRecordingsList {...props} selectedId={selectedId} onSelect={onSelect} />,
  );
  return {
    onSelect,
    rerender: (next: string | null) =>
      view.rerender(<RecentRecordingsList {...props} selectedId={next} onSelect={onSelect} />),
  };
}

describe("RecentRecordingsList rows", () => {
  beforeEach(() => iconRenders.byApp.clear());

  it("re-renders only the rows whose selection changed", () => {
    const { rerender } = renderList("a");
    expect(iconRenders.byApp.get("App-c")).toBe(1);
    rerender("b");
    expect(iconRenders.byApp.get("App-a")).toBe(2);
    expect(iconRenders.byApp.get("App-b")).toBe(2);
    expect(iconRenders.byApp.get("App-c")).toBe(1);
    expect(screen.getByText("text b").closest("button")).toHaveAttribute("aria-current", "true");
  });

  it("keeps click and arrow-key navigation working", () => {
    const { onSelect } = renderList("a");
    fireEvent.click(screen.getByText("text b"));
    expect(onSelect).toHaveBeenCalledWith("b");
    const first = screen.getByText("text a").closest("button") as HTMLButtonElement;
    fireEvent.keyDown(first, { key: "ArrowDown" });
    expect(onSelect).toHaveBeenLastCalledWith("b");
    expect(document.activeElement?.id).toBe("history-item-b");
  });
});
