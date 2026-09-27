import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HotkeyInput } from "./HotkeyInput";
import { checkForSystemConflict } from "@/lib/hotkey-conflicts";

// Mock the platform detection
vi.mock("@/lib/platform", () => ({
  isMacOS: true,
  isWindows: false,
  isLinux: false,
}));

describe("HotkeyInput", () => {
  const mockOnChange = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("displays the current hotkey value", () => {
    render(
      <HotkeyInput
        value="CommandOrControl+Shift+Space"
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />,
    );

    // User should see the hotkey displayed with platform-specific symbols
    expect(screen.getByText("⌘")).toBeInTheDocument();
    expect(screen.getByText("⇧")).toBeInTheDocument();
    expect(screen.getByText("Space")).toBeInTheDocument();
  });

  it("shows placeholder when no hotkey is set", () => {
    render(<HotkeyInput value="" onChange={mockOnChange} placeholder="Click to set hotkey" />);

    expect(screen.getByText("Click to set hotkey")).toBeInTheDocument();
  });

  it("allows user to edit the hotkey", async () => {
    const user = userEvent.setup();

    render(
      <HotkeyInput
        value="CommandOrControl+Space"
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />,
    );

    // User clicks the edit button
    const editButton = screen.getByTitle("Change hotkey");
    await user.click(editButton);

    // User should see they can now set a new hotkey
    expect(screen.getByText("Press keys to set hotkey")).toBeInTheDocument();
    expect(screen.getByTitle("Save hotkey")).toBeInTheDocument();
    expect(screen.getByTitle("Cancel")).toBeInTheDocument();
  });

  it("allows user to cancel editing", async () => {
    const user = userEvent.setup();

    render(
      <HotkeyInput
        value="CommandOrControl+Space"
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />,
    );

    // User starts editing
    const editButton = screen.getByTitle("Change hotkey");
    await user.click(editButton);

    // User decides to cancel
    const cancelButton = screen.getByTitle("Cancel");
    await user.click(cancelButton);

    // Should return to showing the original hotkey
    expect(screen.getByText("⌘")).toBeInTheDocument();
    expect(screen.getByText("Space")).toBeInTheDocument();

    // Should not have changed the value
    expect(mockOnChange).not.toHaveBeenCalled();
  });

  it("prevents saving invalid hotkey combinations", async () => {
    const user = userEvent.setup();

    render(<HotkeyInput value="" onChange={mockOnChange} placeholder="Click to set hotkey" />);

    // User starts editing
    const editButton = screen.getByTitle("Change hotkey");
    await user.click(editButton);

    // User presses just one key (invalid combination)
    await user.keyboard("a");

    // Save button should be disabled and error shown
    await waitFor(() => {
      const saveButton = screen.getByTitle("Save hotkey");
      expect(saveButton).toBeDisabled();
      expect(screen.getByText(/Minimum 2 key\(s\) required/)).toBeInTheDocument();
    });
  });

  it("saves valid hotkey combinations", async () => {
    const user = userEvent.setup();

    render(<HotkeyInput value="" onChange={mockOnChange} placeholder="Click to set hotkey" />);

    // User starts editing
    const editButton = screen.getByTitle("Change hotkey");
    await user.click(editButton);

    // User presses a valid combination (Cmd+A on Mac)
    await user.keyboard("{Meta>}a{/Meta}");

    // User can save the hotkey
    await waitFor(() => {
      const saveButton = screen.getByTitle("Save hotkey");
      expect(saveButton).not.toBeDisabled();
    });

    const saveButton = screen.getByTitle("Save hotkey");
    await user.click(saveButton);

    // Should have called onChange with a normalized value
    await waitFor(() => {
      expect(mockOnChange).toHaveBeenCalled();
    });
  });

  it("captures Cmd+Ctrl combinations on macOS (both modifiers emitted)", async () => {
    const user = userEvent.setup();

    render(<HotkeyInput value="" onChange={mockOnChange} placeholder="Click to set hotkey" />);

    const editButton = screen.getByTitle("Change hotkey");
    await user.click(editButton);

    // Hold both Command and Control, then press K.
    await user.keyboard("{Meta>}{Control>}k{/Control}{/Meta}");

    // The combined combo should validate and be savable.
    await waitFor(() => {
      expect(screen.getByTitle("Save hotkey")).not.toBeDisabled();
    });

    await user.click(screen.getByTitle("Save hotkey"));

    await waitFor(() => {
      expect(mockOnChange).toHaveBeenCalledWith("CommandOrControl+Control+K");
    });
  });

  it("displays platform-specific symbols correctly", () => {
    render(
      <HotkeyInput
        value="CommandOrControl+Alt+Delete"
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />,
    );

    // On Mac, should show Mac symbols
    expect(screen.getByText("⌘")).toBeInTheDocument(); // Command
    expect(screen.getByText("⌥")).toBeInTheDocument(); // Option/Alt
    expect(screen.getByText("⌦")).toBeInTheDocument(); // Delete
  });

  it("allows Escape key to exit edit mode", async () => {
    const user = userEvent.setup();

    render(
      <HotkeyInput
        value="CommandOrControl+Space"
        onChange={mockOnChange}
        placeholder="Click to set hotkey"
      />,
    );

    // User starts editing
    const editButton = screen.getByTitle("Change hotkey");
    await user.click(editButton);

    // User presses Escape to cancel
    await user.keyboard("{Escape}");

    // Should return to display mode
    await waitFor(() => {
      expect(screen.getByTitle("Change hotkey")).toBeInTheDocument();
      expect(screen.getByText("⌘")).toBeInTheDocument();
    });

    expect(mockOnChange).not.toHaveBeenCalled();
  });
});

// ─── Regression: reserved Cmd+Ctrl combos must be flagged regardless of
//     modifier order. The engine emits `CommandOrControl+Control+<key>` while
//     the macOS reserved-shortcut table lists `Control+CommandOrControl+<key>`;
//     the conflict check canonicalizes modifier order before comparing.
describe("checkForSystemConflict — modifier-order independent (macOS)", () => {
  it("flags Cmd+Ctrl+Q (lock screen) in both emit and table modifier order", () => {
    const emitted = checkForSystemConflict("CommandOrControl+Control+Q", "macos");
    const tableOrder = checkForSystemConflict("Control+CommandOrControl+Q", "macos");

    expect(emitted).not.toBeNull();
    expect(tableOrder).not.toBeNull();
    expect(emitted).toEqual(tableOrder);
    expect(emitted?.description).toBe("macOS Lock Screen");
    expect(emitted?.severity).toBe("error");
  });

  it("flags Cmd+Ctrl+Space (emoji picker) in both emit and table modifier order", () => {
    const emitted = checkForSystemConflict("CommandOrControl+Control+Space", "macos");
    const tableOrder = checkForSystemConflict("Control+CommandOrControl+Space", "macos");

    expect(emitted).not.toBeNull();
    expect(tableOrder).not.toBeNull();
    expect(emitted).toEqual(tableOrder);
    expect(emitted?.description).toBe("macOS Emoji Picker");
    expect(emitted?.severity).toBe("warning");
  });

  it("does not over-broaden: modifier identity still matters", () => {
    // Plain Cmd+Q is a distinct (warning) conflict and must not collide with
    // the Cmd+Ctrl+Q lock-screen combo.
    expect(checkForSystemConflict("CommandOrControl+Q", "macos")?.description).toBe(
      "macOS Quit Application",
    );
    // Cmd+Ctrl+K is not reserved at all.
    expect(checkForSystemConflict("CommandOrControl+Control+K", "macos")).toBeNull();
    // Spotlight (Cmd+Space) still detected, unchanged.
    expect(checkForSystemConflict("CommandOrControl+Space", "macos")?.description).toBe(
      "macOS Spotlight Search",
    );
  });
});

describe("HotkeyInput — flags reserved Cmd+Ctrl combos on capture (macOS)", () => {
  const onChange = vi.fn();
  let originalUA: string;

  beforeEach(() => {
    vi.clearAllMocks();
    // checkForSystemConflict detects the platform from navigator.userAgent;
    // force macOS so the reserved-shortcut table is the macOS one.
    originalUA = window.navigator.userAgent;
    Object.defineProperty(window.navigator, "userAgent", {
      value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
      configurable: true,
    });
  });

  afterEach(() => {
    Object.defineProperty(window.navigator, "userAgent", {
      value: originalUA,
      configurable: true,
    });
  });

  it("blocks Cmd+Ctrl+Q as a reserved system hotkey (lock screen)", async () => {
    const user = userEvent.setup();

    render(<HotkeyInput value="" onChange={onChange} placeholder="Click to set hotkey" />);

    await user.click(screen.getByTitle("Change hotkey"));
    await user.keyboard("{Meta>}{Control>}q{/Control}{/Meta}");

    // The reserved (error) conflict must surface and disable Save.
    await waitFor(() => {
      expect(screen.getByText(/reserved by the system: macOS Lock Screen/)).toBeInTheDocument();
    });
    expect(screen.getByTitle("Save hotkey")).toBeDisabled();
  });

  it("warns on Cmd+Ctrl+Space as a conflicting system hotkey (emoji picker)", async () => {
    const user = userEvent.setup();

    render(<HotkeyInput value="" onChange={onChange} placeholder="Click to set hotkey" />);

    await user.click(screen.getByTitle("Change hotkey"));
    // `[Space]` selects the key by CODE ("Space"); `{Space}` would match by the
    // `key` value (a literal " ") and dispatch an unmapped key.
    await user.keyboard("{Meta>}{Control>}[Space]{/Control}{/Meta}");

    await waitFor(() => {
      expect(screen.getByText(/may conflict: macOS Emoji Picker/)).toBeInTheDocument();
    });
  });
});
