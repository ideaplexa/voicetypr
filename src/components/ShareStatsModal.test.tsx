import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { ShareStatsModal } from "@/components/ShareStatsModal";
import { drawShareCard, toShareCardStats } from "@/components/shareCardRenderer";
import { createUsageFixture } from "@/ui-preview/usageFixture";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("@/components/shareCardRenderer", async (original) => ({
  ...(await original<typeof import("@/components/shareCardRenderer")>()),
  drawShareCard: vi.fn(),
}));
const imageDataUrl = "data:image/png;base64,c3RhdHM=";
const stats = toShareCardStats(createUsageFixture());
beforeEach(() => {
  vi.mocked(drawShareCard).mockResolvedValue(imageDataUrl);
  vi.mocked(invoke).mockResolvedValue(null);
  vi.mocked(save).mockResolvedValue("/tmp/stats.png");
});
async function openModal() {
  render(<ShareStatsModal open onOpenChange={vi.fn()} stats={stats} />);
  await screen.findByRole("img", { name: /Share card showing/ });
}
describe("ShareStatsModal", () => {
  it("copies the rendered PNG through the existing API", async () => {
    await openModal();
    await userEvent.click(screen.getByRole("button", { name: "Copy image" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("copy_image_to_clipboard", { imageDataUrl }),
    );
    expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
  });
  it("saves the PNG through the dialog and existing command", async () => {
    await openModal();
    await userEvent.click(screen.getByRole("button", { name: "Save image…" }));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith({
        defaultPath: expect.stringMatching(/^voicetypr-stats-\d+\.png$/),
        filters: [{ name: "Image", extensions: ["png"] }],
      }),
    );
    expect(invoke).toHaveBeenCalledWith("save_image_to_file", {
      imageDataUrl,
      filePath: "/tmp/stats.png",
    });
  });
  it("does not save when the native dialog is cancelled", async () => {
    vi.mocked(save).mockResolvedValue(null);
    await openModal();
    await userEvent.click(screen.getByRole("button", { name: "Save image…" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(invoke).not.toHaveBeenCalled();
  });
  it("opens the X intent with numbers only through the opener plugin", async () => {
    await openModal();
    await userEvent.click(screen.getByRole("button", { name: "Post on X" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("plugin:opener|open_url", expect.anything()),
    );
    expect(vi.mocked(invoke).mock.calls[0]).toEqual(["copy_image_to_clipboard", { imageDataUrl }]);
    const [command, args] = vi.mocked(invoke).mock.calls[1];
    expect(command).toBe("plugin:opener|open_url");
    const url = new URL(String(args && "url" in args ? args.url : ""));
    expect(url.origin + url.pathname).toBe("https://x.com/intent/post");
    expect(url.searchParams.get("text")).toBe(
      `I've dictated ${stats.totalWords.toLocaleString()} words with @voicetypr and skipped ${stats.timeSavedDisplay.replace(" h", "")} h of typing`,
    );
    expect(url.searchParams.get("url")).toBe("https://voicetypr.com");
    expect(args && "with" in args ? args.with : undefined).toBeNull();
  });
  it("offers recovery if the canvas fails", async () => {
    vi.mocked(drawShareCard).mockRejectedValueOnce(new Error("canvas unavailable"));
    render(<ShareStatsModal open onOpenChange={vi.fn()} stats={stats} />);
    await screen.findByRole("alert");
    expect(screen.getByRole("button", { name: "Copy image" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("img", { name: /Share card showing/ })).toBeInTheDocument();
  });
  it("falls back to a download if the save dialog fails", async () => {
    vi.mocked(save).mockRejectedValueOnce(new Error("no dialog"));
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);
    await openModal();
    await userEvent.click(screen.getByRole("button", { name: "Save image…" }));
    await waitFor(() => expect(click).toHaveBeenCalled());
    click.mockRestore();
  });
});

it("does not open X when copying the image fails", async () => {
  await openModal();
  vi.mocked(invoke).mockRejectedValueOnce(new Error("clipboard unavailable"));
  await userEvent.click(screen.getByRole("button", { name: "Post on X" }));
  expect(invoke).toHaveBeenCalledWith("copy_image_to_clipboard", { imageDataUrl });
  expect(invoke).not.toHaveBeenCalledWith("plugin:opener|open_url", expect.anything());
});
