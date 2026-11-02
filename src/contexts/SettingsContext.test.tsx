import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "@/types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { invoke } from "@tauri-apps/api/core";
import { SettingsProvider, useSettings } from "./SettingsContext";

const initialSettings = {
  theme: "system",
  update_channel: "stable",
  onboarding_completed: false,
} as AppSettings;

function Probe() {
  const { settings, updateSettings } = useSettings();
  if (!settings) return null;

  return (
    <>
      <span>{settings.update_channel}</span>
      <button type="button" onClick={() => void updateSettings({ theme: "dark" })}>
        Change theme
      </button>
      <button type="button" onClick={() => void updateSettings({ update_channel: "beta" })}>
        Choose beta
      </button>
      <span>{settings.onboarding_completed ? "complete" : "incomplete"}</span>
      <button
        type="button"
        onClick={() =>
          void updateSettings({ onboarding_completed: true }, { publishAfterSave: true }).catch(
            () => undefined,
          )
        }
      >
        Complete onboarding
      </button>
    </>
  );
}

describe("SettingsContext update channel persistence", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_settings") return initialSettings;
      if (command === "save_settings") return undefined;
      throw new Error(`Unexpected command: ${command}`);
    });
  });

  it("marks only a direct update-channel choice as explicit", async () => {
    render(
      <SettingsProvider>
        <Probe />
      </SettingsProvider>,
    );

    expect(await screen.findByText("stable")).toBeInTheDocument();
    vi.mocked(invoke).mockClear();

    fireEvent.click(screen.getByRole("button", { name: "Change theme" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    const unrelatedArgs = vi.mocked(invoke).mock.calls[0]?.[1] as Record<string, unknown>;
    expect(unrelatedArgs).not.toHaveProperty("updateChannelExplicit");

    vi.mocked(invoke).mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Choose beta" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: expect.objectContaining({ update_channel: "beta" }),
        updateChannelExplicit: true,
      }),
    );
  });

  it("publishes deferred onboarding completion only after save succeeds", async () => {
    let resolveSave: (() => void) | undefined;
    const savePending = new Promise<void>((resolve) => {
      resolveSave = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_settings") return initialSettings;
      if (command === "save_settings") return savePending;
      throw new Error(`Unexpected command: ${command}`);
    });

    render(
      <SettingsProvider>
        <Probe />
      </SettingsProvider>,
    );

    expect(await screen.findByText("incomplete")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Complete onboarding" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(screen.getByText("incomplete")).toBeInTheDocument();
    expect(screen.queryByText("complete")).not.toBeInTheDocument();

    await act(async () => {
      resolveSave?.();
      await savePending;
    });

    expect(await screen.findByText("complete")).toBeInTheDocument();
  });

  it("does not publish rejected onboarding completion and allows retry", async () => {
    let rejectCompletion = true;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_settings") return initialSettings;
      if (command === "save_settings") {
        if (rejectCompletion) throw new Error("settings store unavailable");
        return undefined;
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    render(
      <SettingsProvider>
        <Probe />
      </SettingsProvider>,
    );

    expect(await screen.findByText("incomplete")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Complete onboarding" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", expect.anything()));
    expect(screen.getByText("incomplete")).toBeInTheDocument();

    rejectCompletion = false;
    fireEvent.click(screen.getByRole("button", { name: "Complete onboarding" }));
    expect(await screen.findByText("complete")).toBeInTheDocument();
  });
});

describe("SettingsContext refresh", () => {
  it("causes no consumer re-render when refreshed settings are unchanged", async () => {
    const renders = { count: 0 };
    const latest: { refresh: () => Promise<void>; loading: boolean } = {
      refresh: async () => undefined,
      loading: false,
    };
    let current = { ...initialSettings };
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_settings") return { ...current };
      throw new Error(`Unexpected command: ${command}`);
    });
    function Consumer() {
      const ctx = useSettings();
      useEffect(() => {
        latest.refresh = ctx.refreshSettings;
        latest.loading = ctx.isLoading;
        renders.count += 1; // runs after every commit of this consumer
      });
      return <span>{ctx.settings?.theme ?? "none"}</span>;
    }
    render(
      <SettingsProvider>
        <Consumer />
      </SettingsProvider>,
    );
    await screen.findByText("system");
    // The loaded commit's passive effect may still be pending when the DOM shows
    // "system", so wait for it to report the settled state and flush before counting.
    await waitFor(() => expect(latest.loading).toBe(false));
    await act(async () => {});
    const settled = renders.count;
    await act(async () => latest.refresh());
    expect(renders.count).toBe(settled);
    expect(latest.loading).toBe(false);

    current = { ...current, theme: "dark" };
    await act(async () => latest.refresh());
    await screen.findByText("dark");
    expect(renders.count).toBe(settled + 1);
  });
});
