import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ShortcutsSection } from "../ShortcutsSection";
import type { ShortcutActionDefinition, ShortcutSettings } from "@/types/shortcuts";
import { toast } from "sonner";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@/lib/platform", () => ({
  isMacOS: true,
  isWindows: false,
  isLinux: false,
}));

vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
  },
}));

const actionDefinitions: ShortcutActionDefinition[] = [
  {
    action: "toggle_recording",
    label: "Toggle Recording",
    description: "Start or stop recording from anywhere.",
    section: "Recording",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "hold_to_record",
    label: "Hold to Record",
    description: "Record only while the shortcut is held.",
    section: "Recording",
    recommended_trigger: "hold",
    allows_single_key: true,
  },
  {
    action: "cancel_recording",
    label: "Cancel Recording",
    description: "Cancel the current recording.",
    section: "Recording",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "copy_last_transcription",
    label: "Copy Last Transcription",
    description: "Copy the most recent transcript to the clipboard.",
    section: "History",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "paste_last_transcription",
    label: "Paste Last Transcription",
    description: "Paste the most recent transcript.",
    section: "History",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "toggle_ai_formatting",
    label: "Toggle Polish",
    description: "Turn Polish on or off.",
    section: "Polish",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
  {
    action: "open_dashboard",
    label: "Open Dashboard",
    description: "Show the Voicetypr dashboard.",
    section: "App",
    recommended_trigger: "pressed",
    allows_single_key: true,
  },
];

function arrangeInvoke(
  settings: ShortcutSettings = { bindings: [] },
  onUpdate: (
    submittedSettings: ShortcutSettings,
  ) => ShortcutSettings | Promise<ShortcutSettings> = (submittedSettings) => submittedSettings,
  options: { rejectSettings?: Error } = {},
) {
  const invokeMock = vi.mocked(invoke);

  invokeMock.mockImplementation((command: string, args?: unknown) => {
    if (command === "list_shortcut_actions") {
      return Promise.resolve(actionDefinitions);
    }

    if (command === "get_shortcut_settings") {
      return options.rejectSettings
        ? Promise.reject(options.rejectSettings)
        : Promise.resolve(settings);
    }

    if (command === "update_shortcut_settings") {
      return Promise.resolve(onUpdate((args as { settings: ShortcutSettings }).settings)).then(
        (updatedSettings) => {
          settings = updatedSettings;
          return settings;
        },
      );
    }

    return Promise.resolve(undefined);
  });
}

describe("ShortcutsSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    arrangeInvoke();
  });

  it("loads action rows with empty default settings", async () => {
    render(<ShortcutsSection />);

    await waitFor(() => {
      expect(screen.getByRole("heading", { name: "Recording" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "History" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "Polish" })).toBeInTheDocument();
      expect(screen.getByRole("heading", { name: "App" })).toBeInTheDocument();
    });

    // Toggle/Hold Recording are the PRIMARY recording trigger and are managed in
    // General Settings, so they must NOT appear here. Cancel Recording stays.
    expect(screen.queryByText("Toggle Recording")).not.toBeInTheDocument();
    expect(screen.queryByText("Hold to Record")).not.toBeInTheDocument();
    expect(screen.getByText("Cancel Recording")).toBeInTheDocument();
    expect(
      screen.getByText("Press Escape twice while recording to cancel the current take."),
    ).toBeInTheDocument();
    expect(screen.getByText("Copy Last Transcription")).toBeInTheDocument();
    expect(screen.getByText("Toggle Polish")).toBeInTheDocument();
    expect(screen.getByText("Open Dashboard")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Set shortcut" })).toHaveLength(
      actionDefinitions.filter(
        (a) => a.action !== "toggle_recording" && a.action !== "hold_to_record",
      ).length,
    );
    expect(screen.queryByText("0 of 5 single-key shortcuts used.")).not.toBeInTheDocument();
    expect(screen.queryByText("0 bindings configured")).not.toBeInTheDocument();
  });

  it("adds a copy-last shortcut and sends the full settings object", async () => {
    const user = userEvent.setup();
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    await user.click(within(copyRow).getByRole("button", { name: "Set shortcut" }));
    await user.keyboard("{Alt>}{Shift>}c{/Shift}{/Alt}");
    await user.click(within(copyRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              action: "copy_last_transcription",
              shortcut: "Alt+Shift+C",
              trigger: "pressed",
              enabled: true,
              allow_risky_combo: false,
            }),
          ],
        },
      });
    });
  });

  it("blocks duplicate shortcuts before saving and names the assigned action", async () => {
    const user = userEvent.setup();
    arrangeInvoke({
      bindings: [
        {
          id: "copy-binding",
          action: "copy_last_transcription",
          shortcut: "Alt+C",
          trigger: "pressed",
          enabled: true,
          allow_risky_combo: false,
        },
      ],
    });

    render(<ShortcutsSection />);

    const pasteRow = await screen.findByRole("group", { name: "Paste Last Transcription" });
    await user.click(within(pasteRow).getByRole("button", { name: "Set shortcut" }));
    await user.keyboard("{Alt>}c{/Alt}");
    await user.click(within(pasteRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Shortcut already assigned", {
        description: "Alt+C is already assigned to Copy Last Transcription.",
      });
    });
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "update_shortcut_settings"),
    ).toHaveLength(0);
  });

  it("allows reusing a shortcut from a disabled binding", async () => {
    const user = userEvent.setup();
    arrangeInvoke({
      bindings: [
        {
          id: "copy-binding",
          action: "copy_last_transcription",
          shortcut: "Alt+C",
          trigger: "pressed",
          enabled: false,
          allow_risky_combo: false,
        },
      ],
    });

    render(<ShortcutsSection />);

    const pasteRow = await screen.findByRole("group", { name: "Paste Last Transcription" });
    await user.click(within(pasteRow).getByRole("button", { name: "Set shortcut" }));
    await user.keyboard("{Alt>}c{/Alt}");
    await user.click(within(pasteRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              id: "copy-binding",
              enabled: false,
            }),
            expect.objectContaining({
              action: "paste_last_transcription",
              shortcut: "Alt+C",
              enabled: true,
            }),
          ],
        },
      });
    });
    expect(toast.error).not.toHaveBeenCalledWith("Shortcut already assigned", expect.anything());
  });

  it("uses the shortcut settings returned by the backend after saving", async () => {
    const user = userEvent.setup();
    arrangeInvoke({ bindings: [] }, (submittedSettings) => ({
      bindings: submittedSettings.bindings.map((binding) => ({
        ...binding,
        shortcut: "Alt+R",
      })),
    }));

    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    await user.click(within(copyRow).getByRole("button", { name: "Set shortcut" }));
    await user.keyboard("{Alt>}{Shift>}c{/Shift}{/Alt}");
    await user.click(within(copyRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      // Backend returned "Alt+R"; verify the read-mode display reflects it (not the submitted "Alt+Shift+C")
      const display = within(copyRow).getByLabelText("Copy Last Transcription shortcut");
      expect(display).toHaveTextContent("Alt+R");
      expect(display).not.toHaveTextContent("Shift");
    });
  });

  it("does not show a single-key toggle on a non-recording action", async () => {
    const user = userEvent.setup();
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    await user.click(within(copyRow).getByRole("button", { name: "Set shortcut" }));

    expect(
      within(copyRow).queryByRole("switch", { name: "Use a single key" }),
    ).not.toBeInTheDocument();
    expect(
      within(copyRow).queryByRole("switch", { name: "Hold to talk (push-to-talk)" }),
    ).not.toBeInTheDocument();
  });

  it("saves a single-key F1 binding on a non-recording action", async () => {
    const user = userEvent.setup();
    arrangeInvoke({ bindings: [] });
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    await user.click(within(copyRow).getByRole("button", { name: "Set shortcut" }));

    fireEvent.keyDown(window, { key: "F1", code: "F1" });
    await waitFor(() => {
      expect(within(copyRow).getByRole("button", { name: "Save" })).toBeEnabled();
    });
    await user.click(within(copyRow).getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: {
          bindings: [
            expect.objectContaining({
              action: "copy_last_transcription",
              shortcut: "F1",
              allow_risky_combo: true,
            }),
          ],
        },
      });
    });
  });

  it("removes an existing binding via Remove (no delete button or enable switch)", async () => {
    const user = userEvent.setup();
    arrangeInvoke({
      bindings: [
        {
          id: "copy-binding",
          action: "copy_last_transcription",
          shortcut: "Alt+C",
          trigger: "pressed",
          enabled: true,
          allow_risky_combo: false,
        },
      ],
    });
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    expect(within(copyRow).queryByRole("button", { name: /Delete/ })).not.toBeInTheDocument();
    expect(within(copyRow).queryByRole("switch", { name: /enabled/ })).not.toBeInTheDocument();

    await user.click(within(copyRow).getByRole("button", { name: "Remove" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_shortcut_settings", {
        settings: { bindings: [] },
      });
    });
  });

  it("offers only one shortcut slot per action (no add-another button once set)", async () => {
    arrangeInvoke({
      bindings: [
        {
          id: "copy-binding",
          action: "copy_last_transcription",
          shortcut: "Alt+C",
          trigger: "pressed",
          enabled: true,
          allow_risky_combo: false,
        },
      ],
    });
    render(<ShortcutsSection />);

    const copyRow = await screen.findByRole("group", { name: "Copy Last Transcription" });
    expect(within(copyRow).queryByRole("button", { name: "Set shortcut" })).not.toBeInTheDocument();
    expect(within(copyRow).queryByRole("button", { name: "Add shortcut" })).not.toBeInTheDocument();
    expect(within(copyRow).getByRole("button", { name: "Edit" })).toBeInTheDocument();
  });
});
