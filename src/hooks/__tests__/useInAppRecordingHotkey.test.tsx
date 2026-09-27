import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

const platform = vi.hoisted(() => ({ isMacOS: false }));
vi.mock("@/lib/platform", () => ({
  get isMacOS() {
    return platform.isMacOS;
  },
  get isWindows() {
    return !platform.isMacOS;
  },
}));

const mockRecording = vi.hoisted(() => ({
  state: "idle" as string,
  error: null as string | null,
  isActive: false,
  startRecording: vi.fn(),
  stopRecording: vi.fn(),
}));
vi.mock("@/hooks/useRecording", () => ({
  useRecording: () => mockRecording,
}));

const mockSettings = vi.hoisted(
  () => ({ hotkey: "CommandOrControl+Space" }) as Record<string, unknown>,
);
vi.mock("@/contexts/SettingsContext", () => ({
  useSetting: (key: string) => mockSettings[key],
}));

const mockInvoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const eventMock = vi.hoisted(() => ({
  shortcutSettingsChangedHandler: null as null | (() => void),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: () => void) => {
    if (event === "shortcut-settings-changed") {
      eventMock.shortcutSettingsChangedHandler = handler;
    }
    return Promise.resolve(() => {});
  }),
}));

import { useInAppRecordingHotkey } from "@/hooks/useInAppRecordingHotkey";

function fireHotkey(target: Element, init: KeyboardEventInit = {}): void {
  target.dispatchEvent(
    new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      ctrlKey: true,
      code: "Space",
      key: " ",
      ...init,
    }),
  );
}

// A bare-modifier isolated-tap primary (e.g. Control alone), as returned by
// get_shortcut_settings when no combo hotkey is configured.
const bareControlBinding = {
  id: "onboarding-primary-hold",
  action: "toggle_recording",
  shortcut: "",
  trigger: "pressed", // pass-through struct field; tapToggleModifier filters on trigger_kind/action only
  enabled: true,
  allow_risky_combo: false,
  trigger_kind: "isolated_tap",
  modifier: { modifier: "control", side: "either" },
};

const holdControlBinding = {
  ...bareControlBinding,
  action: "hold_to_record",
  trigger: "hold",
  trigger_kind: "modifier_hold",
};

// Dispatch a clean lone-modifier tap (keydown then keyup, same physical key).
function fireModifierTap(target: Element, init: KeyboardEventInit = {}): void {
  const opts = { bubbles: true, cancelable: true, code: "ControlLeft", key: "Control", ...init };
  target.dispatchEvent(new KeyboardEvent("keydown", opts));
  target.dispatchEvent(new KeyboardEvent("keyup", opts));
}

function fireModifierDown(target: Element, init: KeyboardEventInit = {}): void {
  target.dispatchEvent(
    new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      code: "ControlLeft",
      key: "Control",
      ...init,
    }),
  );
}

function fireModifierUp(target: Element, init: KeyboardEventInit = {}): void {
  target.dispatchEvent(
    new KeyboardEvent("keyup", {
      bubbles: true,
      cancelable: true,
      code: "ControlLeft",
      key: "Control",
      ...init,
    }),
  );
}

// Render the hook and flush the async get_shortcut_settings load so the
// bare-modifier binding is in place before dispatching events.
async function renderWithBareModifier(): Promise<void> {
  renderHook(() => useInAppRecordingHotkey());
  await act(async () => {
    // Two microtask flushes: the invoke() promise resolves on the first, its
    // .then() (which sets the bare-modifier ref) on the second.
    await Promise.resolve();
    await Promise.resolve();
  });
}

async function flushHoldStart(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => window.setTimeout(resolve, 0));
  });
}

function markAltGraph(event: KeyboardEvent): KeyboardEvent {
  Object.defineProperty(event, "getModifierState", {
    value: (key: string) => key === "AltGraph",
  });
  return event;
}

describe("useInAppRecordingHotkey", () => {
  let editable: HTMLTextAreaElement;
  let nonEditable: HTMLDivElement;

  beforeEach(() => {
    platform.isMacOS = false;
    mockSettings.hotkey = "CommandOrControl+Space";
    mockRecording.isActive = false;
    mockRecording.state = "idle";
    mockRecording.startRecording.mockReset();
    mockRecording.stopRecording.mockReset();
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue({ bindings: [] });
    eventMock.shortcutSettingsChangedHandler = null;
    editable = document.createElement("textarea");
    nonEditable = document.createElement("div");
    document.body.append(editable, nonEditable);
  });

  afterEach(() => {
    editable.remove();
    nonEditable.remove();
  });

  it("starts recording when the hotkey fires inside a text field", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("stops recording when it is already recording", () => {
    mockRecording.state = "recording";
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);

    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores the hotkey during transitional states (transcribing)", () => {
    mockRecording.state = "transcribing";
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("ignores the hotkey when focus is not in an editable field", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(nonEditable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("ignores a combo that does not match the configured hotkey", () => {
    renderHook(() => useInAppRecordingHotkey());

    // Missing the Control modifier → plain Space.
    fireHotkey(editable, { ctrlKey: false });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("debounces a rapid second press within the window", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);
    fireHotkey(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
  });

  it("ignores auto-repeat keydowns", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable, { repeat: true });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores IME composition events", () => {
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable, { isComposing: true });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("does nothing when no hotkey is configured", () => {
    mockSettings.hotkey = "";
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("leaves a bare-key hotkey to the field so typing still works", () => {
    mockSettings.hotkey = "Space";
    renderHook(() => useInAppRecordingHotkey());

    const event = new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      code: "Space",
      key: " ",
    });
    const preventDefault = vi.spyOn(event, "preventDefault");
    editable.dispatchEvent(event);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(preventDefault).not.toHaveBeenCalled();
  });

  it("ignores Shift-only combos that can produce typed characters", () => {
    mockSettings.hotkey = "Shift+Space";
    renderHook(() => useInAppRecordingHotkey());

    fireHotkey(editable, { ctrlKey: false, shiftKey: true });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores AltGr keystrokes so typing isn't stolen (Ctrl+Alt + AltGraph)", () => {
    mockSettings.hotkey = "CommandOrControl+Alt+Q";
    renderHook(() => useInAppRecordingHotkey());

    const event = new KeyboardEvent("keydown", {
      bubbles: true,
      cancelable: true,
      ctrlKey: true,
      altKey: true,
      code: "KeyQ",
      key: "@",
    });
    // jsdom doesn't honor modifierAltGraph init reliably; force AltGraph on.
    Object.defineProperty(event, "getModifierState", {
      value: (key: string) => key === "AltGraph",
    });
    const preventDefault = vi.spyOn(event, "preventDefault");
    editable.dispatchEvent(event);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
    expect(preventDefault).not.toHaveBeenCalled();
  });

  it("starts recording on a bare-modifier tap inside a text field", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("stops recording on a bare-modifier tap when already recording", async () => {
    mockSettings.hotkey = "";
    mockRecording.state = "recording";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("ignores a chord (Ctrl+C) — a key pressed during the modifier hold", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    const opts = { bubbles: true, cancelable: true };
    editable.dispatchEvent(
      new KeyboardEvent("keydown", { ...opts, code: "ControlLeft", key: "Control", ctrlKey: true }),
    );
    editable.dispatchEvent(
      new KeyboardEvent("keydown", { ...opts, code: "KeyC", key: "c", ctrlKey: true }),
    );
    editable.dispatchEvent(
      new KeyboardEvent("keyup", { ...opts, code: "KeyC", key: "c", ctrlKey: true }),
    );
    editable.dispatchEvent(
      new KeyboardEvent("keyup", { ...opts, code: "ControlLeft", key: "Control" }),
    );

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("ignores a bare-modifier tap outside an editable field", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(nonEditable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("respects the configured modifier side", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [{ ...bareControlBinding, modifier: { modifier: "control", side: "left" } }],
    });
    await renderWithBareModifier();

    // Right Control when only Left is configured → no toggle.
    fireModifierTap(editable, { code: "ControlRight" });

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("starts on keydown and stops on keyup for a push-to-talk modifier_hold binding", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    await renderWithBareModifier();

    fireModifierDown(editable);
    await flushHoldStart();

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();

    mockRecording.state = "recording";
    fireModifierUp(editable);

    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
  });

  it("suppresses auto-repeat keydowns for a push-to-talk modifier_hold binding", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    await renderWithBareModifier();

    fireModifierDown(editable);
    fireModifierDown(editable, { repeat: true });
    fireModifierDown(editable, { repeat: true });
    await flushHoldStart();

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
  });

  it("does not start a modifier_hold recording for AltGr's synthesized Control then RightAlt sequence", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    await renderWithBareModifier();

    fireModifierDown(editable);
    editable.dispatchEvent(
      markAltGraph(
        new KeyboardEvent("keydown", {
          bubbles: true,
          cancelable: true,
          code: "AltRight",
          key: "AltGraph",
          ctrlKey: true,
          altKey: true,
        }),
      ),
    );
    await flushHoldStart();
    editable.dispatchEvent(
      markAltGraph(
        new KeyboardEvent("keyup", {
          bubbles: true,
          cancelable: true,
          code: "ControlLeft",
          key: "Control",
          ctrlKey: true,
          altKey: true,
        }),
      ),
    );

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("chains the hold stop after an in-flight start so stop-before-Starting is never dropped", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    let resolveStart: ((started: boolean) => void) | undefined;
    mockRecording.startRecording.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveStart = resolve;
        }),
    );
    await renderWithBareModifier();

    fireModifierDown(editable);
    await flushHoldStart();
    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);

    // Key released while the start invoke is still in flight (backend has not
    // yet published `Starting`): the stop must wait for the start, not drop.
    // NOTE: React state deliberately stays 'idle' here — the settle-time gate
    // must rely on the reported outcome, not possibly-stale observed state.
    fireModifierUp(editable);
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();

    resolveStart?.(true);
    await Promise.resolve();
    await Promise.resolve();
    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
  });

  it("keyup still stops the hold after a duplicate non-repeat keydown of the same modifier", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    mockRecording.startRecording.mockResolvedValueOnce(true);
    await renderWithBareModifier();

    fireModifierDown(editable);
    await flushHoldStart();
    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    mockRecording.state = "recording";

    // Duplicate keydown WITHOUT repeat (lost keyup / focus churn) must not
    // clobber the dispatched hold state or re-arm the start.
    fireModifierDown(editable);
    await flushHoldStart();
    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);

    fireModifierUp(editable);
    await Promise.resolve();
    await Promise.resolve();
    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
  });

  it("does not issue the deferred stop when the start was a redundant no-op on a recording it does not own", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    let resolveStart: ((started: boolean) => void) | undefined;
    mockRecording.startRecording.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveStart = resolve;
        }),
    );
    await renderWithBareModifier();

    fireModifierDown(editable);
    await flushHoldStart();
    fireModifierUp(editable);

    // Backend reports false: a recording was already starting/active (e.g. the
    // native hotkey path won the race), so this hold owns nothing — stopping
    // would kill someone else's dictation mid-sentence.
    resolveStart?.(false);
    await Promise.resolve();
    await Promise.resolve();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("does not issue the deferred stop when the in-flight start failed, preserving the error state", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    let resolveStart: ((started: boolean) => void) | undefined;
    mockRecording.startRecording.mockImplementationOnce(
      () =>
        new Promise<boolean>((resolve) => {
          resolveStart = resolve;
        }),
    );
    await renderWithBareModifier();

    fireModifierDown(editable);
    await flushHoldStart();
    fireModifierUp(editable);

    // Start settles as a failure (startRecording swallows the rejection and
    // reports false). The deferred stop must NOT fire — it would reset the
    // backend to Idle and wipe the error message.
    resolveStart?.(false);
    await Promise.resolve();
    await Promise.resolve();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("stops a hold that already started when AltGr's second key arrives after the hold-start timer", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    await renderWithBareModifier();

    fireModifierDown(editable);
    // Race: the 0ms hold-start timer fires BETWEEN the synthesized Control
    // keydown and the AltGraph keydown — the hold has already started.
    await flushHoldStart();
    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    mockRecording.state = "recording";
    editable.dispatchEvent(
      markAltGraph(
        new KeyboardEvent("keydown", {
          bubbles: true,
          cancelable: true,
          code: "AltRight",
          key: "AltGraph",
          ctrlKey: true,
          altKey: true,
        }),
      ),
    );

    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
  });

  it("stops an active modifier_hold even when keyup still reports AltGraph", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({
      bindings: [holdControlBinding],
    });
    await renderWithBareModifier();

    fireModifierDown(editable);
    await flushHoldStart();
    mockRecording.state = "recording";
    editable.dispatchEvent(
      markAltGraph(
        new KeyboardEvent("keyup", {
          bubbles: true,
          cancelable: true,
          code: "ControlLeft",
          key: "Control",
          ctrlKey: true,
          altKey: true,
        }),
      ),
    );

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.stopRecording).toHaveBeenCalledTimes(1);
  });

  it("bails if recording state changed between keydown and keyup", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    const opts = { bubbles: true, cancelable: true, code: "ControlLeft", key: "Control" };
    editable.dispatchEvent(new KeyboardEvent("keydown", opts));
    mockRecording.state = "recording"; // native fired in between
    editable.dispatchEvent(new KeyboardEvent("keyup", opts));

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("does not activate the bare-modifier fallback when get_shortcut_settings fails", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockRejectedValue(new Error("backend unavailable"));
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("clears a pending tap on blur (no toggle on a later keyup)", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    const opts = { bubbles: true, cancelable: true, code: "ControlLeft", key: "Control" };
    editable.dispatchEvent(new KeyboardEvent("keydown", opts));
    window.dispatchEvent(new Event("blur"));
    editable.dispatchEvent(new KeyboardEvent("keyup", opts));

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("debounces a rapid second bare-modifier tap", async () => {
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(editable);
    fireModifierTap(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
  });

  it("does not run the bare-modifier path on macOS (native engine handles it)", async () => {
    platform.isMacOS = true;
    mockSettings.hotkey = "";
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
  });

  it("reloads the bare-modifier binding when shortcut settings change in-session", async () => {
    mockSettings.hotkey = "";
    // Initially no bare binding → a tap does nothing.
    mockInvoke.mockResolvedValue({ bindings: [] });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();

    // Backend now reports a bare-Control isolated_tap binding (in-session save).
    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await act(async () => {
      eventMock.shortcutSettingsChangedHandler?.();
      // Flush the reload's invoke().then() that sets the bare-modifier ref.
      await Promise.resolve();
      await Promise.resolve();
    });

    fireModifierTap(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
    expect(mockRecording.stopRecording).not.toHaveBeenCalled();
  });

  it("rearms from shortcut-settings-changed even when the cached hotkey is stale", async () => {
    mockSettings.hotkey = "CommandOrControl+Shift+Space";
    mockInvoke.mockResolvedValue({ bindings: [] });
    await renderWithBareModifier();

    fireModifierTap(editable);

    expect(mockRecording.startRecording).not.toHaveBeenCalled();

    mockInvoke.mockResolvedValue({ bindings: [bareControlBinding] });
    await act(async () => {
      eventMock.shortcutSettingsChangedHandler?.();
      await Promise.resolve();
      await Promise.resolve();
    });

    fireModifierTap(editable);

    expect(mockRecording.startRecording).toHaveBeenCalledTimes(1);
  });
});
