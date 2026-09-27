import { StrictMode, type MutableRefObject } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { usePolishSettingsLoad } from "./usePolishSettingsLoad";
import type { AISettings } from "@/types/ai";

interface LoadProps {
  settingsLoaded: boolean;
  setSettingsLoaded: (loaded: boolean) => void;
  loadAISettings: (signal?: AbortSignal) => Promise<AISettings | null | undefined>;
  loadEnhancementOptionsRef: MutableRefObject<
    (aiEnabled: boolean, signal?: AbortSignal) => Promise<void>
  >;
  loadWritingSettingsRef: MutableRefObject<(signal?: AbortSignal) => Promise<boolean>>;
}

const loadedSettings: AISettings = {
  enabled: false,
  provider: "",
  model: "",
  hasApiKey: false,
  modelsByProvider: {},
  reasoningByProvider: {},
  fastModeByProvider: {},
};

const resolvedSettingsLoad = (): Promise<AISettings | null | undefined> =>
  Promise.resolve(loadedSettings);

function makeProps(overrides: Partial<LoadProps> = {}): LoadProps {
  return {
    settingsLoaded: false,
    setSettingsLoaded: vi.fn(),
    loadAISettings: vi.fn(resolvedSettingsLoad),
    loadEnhancementOptionsRef: { current: vi.fn(async () => {}) },
    loadWritingSettingsRef: { current: vi.fn(async () => true) },
    ...overrides,
  };
}

function renderLoad(props: LoadProps) {
  return renderHook((next: LoadProps) => usePolishSettingsLoad(next), {
    wrapper: StrictMode,
    initialProps: props,
  });
}

describe("usePolishSettingsLoad", () => {
  it("completes the load in StrictMode when the first attempt is canceled", async () => {
    // StrictMode mounts, cleans up, and remounts the effect. The canceled
    // first attempt must not leave the start guard raised: the replacement
    // effect has to be able to run the load to completion.
    const props = makeProps();

    renderLoad(props);

    await waitFor(() => {
      expect(props.setSettingsLoaded).toHaveBeenCalledWith(true);
    });
    // The surviving attempt — not the canceled one — performs the side effects.
    expect(props.loadEnhancementOptionsRef.current).toHaveBeenCalledTimes(1);
    expect(props.loadWritingSettingsRef.current).toHaveBeenCalledTimes(1);
  });

  it("discards a superseded attempt's results", async () => {
    let resolveSuspended: ((value: AISettings | null | undefined) => void) | undefined;
    const suspendedLoad = () =>
      new Promise<AISettings | null | undefined>((resolve) => {
        resolveSuspended = resolve;
      });
    const loadAISettings = vi
      .fn((): Promise<AISettings | null | undefined> => Promise.resolve(loadedSettings))
      .mockImplementationOnce(suspendedLoad);
    const props = makeProps({ loadAISettings });

    renderLoad(props);

    // The replacement attempt finishes while the first attempt is suspended.
    await waitFor(() => {
      expect(props.setSettingsLoaded).toHaveBeenCalledWith(true);
    });

    await act(async () => {
      resolveSuspended?.(loadedSettings);
    });

    // The stale result applies nothing: no side effects run a second time and
    // the completion is reported once.
    expect(props.loadEnhancementOptionsRef.current).toHaveBeenCalledTimes(1);
    expect(props.loadWritingSettingsRef.current).toHaveBeenCalledTimes(1);
    expect(props.setSettingsLoaded).toHaveBeenCalledTimes(1);
  });

  it("keeps completed background work alive until unmount", async () => {
    let signal: AbortSignal | undefined;
    const props = makeProps({
      loadAISettings: async (nextSignal) => {
        signal = nextSignal;
        return loadedSettings;
      },
    });
    const { rerender, unmount } = renderLoad(props);
    await waitFor(() => expect(props.setSettingsLoaded).toHaveBeenCalledWith(true));
    rerender({ ...props, settingsLoaded: true });
    expect(signal?.aborted).toBe(false);
    unmount();
    expect(signal?.aborted).toBe(true);
  });

  it("retries and reports the loaded state after a failed attempt", async () => {
    const initial = makeProps({
      loadAISettings: vi.fn(() => Promise.reject(new Error("load failed"))),
    });
    const { rerender } = renderLoad(initial);

    // Let the failed attempt settle: the rejection is handled inside the
    // hook and completion is never reported to the consumer.
    await act(async () => {});
    expect(initial.setSettingsLoaded).not.toHaveBeenCalled();

    // A replacement attempt (here driven by a dependency change) starts from
    // a clean guard and the consumer observes the loaded state.
    const retried = makeProps();
    rerender(retried);

    await waitFor(() => {
      expect(retried.setSettingsLoaded).toHaveBeenCalledWith(true);
    });
  });
});
