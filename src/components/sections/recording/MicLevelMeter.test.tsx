import { act, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MicLevelMeter } from "@/components/sections/recording/CaptureControlsCard";
import { readFileSync } from "node:fs";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn().mockResolvedValue(undefined),
  isVisible: vi.fn<() => Promise<boolean>>(),
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  unlisten: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ isVisible: mocks.isVisible }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: (event: { payload: unknown }) => void) => {
    mocks.handlers.set(event, handler);
    return () => {
      mocks.handlers.delete(event);
      mocks.unlisten();
    };
  }),
}));

function emit(event: string, payload: unknown) {
  act(() => mocks.handlers.get(event)?.({ payload }));
}

describe("MicLevelMeter native visibility", () => {
  beforeEach(() => {
    mocks.handlers.clear();
    mocks.isVisible.mockResolvedValue(true);
    vi.stubGlobal(
      "requestAnimationFrame",
      vi.fn(() => 42),
    );
    vi.stubGlobal("cancelAnimationFrame", vi.fn());
  });
  afterEach(() => vi.unstubAllGlobals());

  it("wires successful native hide/show to backend emission gating", () => {
    const read = (file: string) => readFileSync(`src-tauri/src/${file}`, "utf8");
    const lib = read("lib.rs");
    const hide = lib.slice(
      lib.indexOf("fn hide_main_window"),
      lib.indexOf("// Read the Windows taskbar"),
    );
    const show = lib.slice(
      lib.indexOf("fn show_main_window"),
      lib.indexOf("fn cloud_selection_is_kept"),
    );
    expect(hide).toContain("main_window_visibility_changed(&window, false)");
    expect(show).toContain("main_window_visibility_changed(&window, true)");
    expect(read("commands/window.rs")).toContain(
      "main_window_visibility_changed(&main_window, true)",
    );
    expect(read("pill/level.rs")).toContain("MAIN_WINDOW_VISIBLE.load(Ordering::Relaxed)");
  });

  it("stops IPC and pending paint on hide, then resumes on show without unmounting", async () => {
    const { unmount } = render(<MicLevelMeter />);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("set_audio_meter_active", { active: true }),
    );
    emit("audio-level", { level: 0.8 });
    expect(requestAnimationFrame).toHaveBeenCalledTimes(1);
    emit("main-window-visibility-changed", false);
    expect(mocks.invoke).toHaveBeenLastCalledWith("set_audio_meter_active", { active: false });
    expect(cancelAnimationFrame).toHaveBeenCalledWith(42);
    emit("audio-level", { level: 0.9 });
    expect(requestAnimationFrame).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("meter")).toHaveAttribute("aria-valuenow", "0");
    emit("main-window-visibility-changed", true);
    expect(mocks.invoke).toHaveBeenLastCalledWith("set_audio_meter_active", { active: true });
    emit("audio-level", 0.6);
    expect(requestAnimationFrame).toHaveBeenCalledTimes(2);
    unmount();
    expect(mocks.unlisten).toHaveBeenCalledTimes(2);
    expect(mocks.invoke).toHaveBeenLastCalledWith("set_audio_meter_active", { active: false });
  });

  it("does not let a stale native snapshot reactivate a hidden meter", async () => {
    let resolveVisibility!: (visible: boolean) => void;
    mocks.isVisible.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveVisibility = resolve;
        }),
    );
    render(<MicLevelMeter />);
    await waitFor(() => expect(mocks.isVisible).toHaveBeenCalled());
    emit("main-window-visibility-changed", false);
    await act(async () => resolveVisibility(true));
    expect(mocks.invoke).not.toHaveBeenCalledWith("set_audio_meter_active", { active: true });
    emit("audio-level", 1);
    expect(requestAnimationFrame).not.toHaveBeenCalled();
  });

  it("starts inactive when mounted in a hidden native window", async () => {
    mocks.isVisible.mockResolvedValueOnce(false);
    render(<MicLevelMeter />);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("set_audio_meter_active", { active: false }),
    );
    emit("audio-level", 1);
    expect(requestAnimationFrame).not.toHaveBeenCalled();
  });
});
