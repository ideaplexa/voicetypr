import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WhatsNewDialog } from "@/components/WhatsNewDialog";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());

const version = "2.1.0-beta.6";
const url = `https://github.com/ideaplexa/voicetypr/releases/tag/v${version}`;
const fallback = "This update includes fixes and improvements.";
const show = (body: string | null) => {
  vi.mocked(invoke).mockResolvedValue({ version, body });
  return render(<WhatsNewDialog version={version} onClose={() => {}} />);
};

it("offers the installed-version release page when offline", async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_release_notes") throw new Error("offline");
    return undefined;
  });
  render(<WhatsNewDialog version={version} onClose={() => {}} />);
  expect(await screen.findByText(fallback)).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "See full release notes" })).toHaveAttribute("href", url);
});

it("renders manifest highlights as bullets and opens links externally", async () => {
  show("- Better dictation\n- Faster startup\n- [Microphone recovery](https://example.com/fix)");
  expect(await screen.findAllByRole("listitem")).toHaveLength(3);
  fireEvent.click(screen.getByRole("link", { name: "Microphone recovery" }));
  expect(invoke).toHaveBeenCalledWith("plugin:opener|open_url", {
    url: "https://example.com/fix", with: null,
  });
  fireEvent.click(screen.getByRole("link", { name: "See full release notes" }));
  expect(invoke).toHaveBeenCalledWith("plugin:opener|open_url", { url, with: null });
});

it.each(["See the release notes for v2.1.0-beta.6", "  See the full release notes…  ", "", " \n ", null])(
  "uses a friendly overview for placeholder or empty notes: %s",
  async (body) => {
    show(body);
    expect(await screen.findByText(fallback)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "See full release notes" })).toHaveAttribute("href", url);
    expect(screen.queryByText("See the release notes for v2.1.0-beta.6")).toBeNull();
  },
);

it("normalizes an installed version with a leading v", async () => {
  vi.mocked(invoke).mockResolvedValue({ version, body: "- Better dictation" });
  render(<WhatsNewDialog version={`v${version}`} onClose={() => {}} />);
  expect(await screen.findByRole("listitem")).toHaveTextContent("Better dictation");
  expect(screen.getByRole("link", { name: "See full release notes" })).toHaveAttribute("href", url);
});

it("keeps bad, very long notes in a bounded, keyboard-scrollable area", async () => {
  show(Array.from({ length: 200 }, (_, index) => `- Improvement ${index}`).join("\n"));
  expect(await screen.findAllByRole("listitem")).toHaveLength(200);
  const region = screen.getByRole("region", { name: "Release highlights" });
  expect(region).toHaveClass("max-h-[45vh]", "min-h-0", "overflow-y-auto", "overflow-x-hidden");
  expect(region).toHaveAttribute("tabindex", "0");
  expect(screen.getByRole("dialog")).toHaveClass("max-h-[85dvh]");
  expect(region).not.toContainElement(screen.getByRole("link", { name: "See full release notes" }));
});

it("renders markdown without raw HTML, remote images or unsafe links", async () => {
  show("## Fixes\n\n**Better** dictation\n\n<script>alert(1)</script>\n\n[bad](javascript:alert%281%29)\n\n![tracking](https://example.com/pixel)");
  expect(await screen.findByRole("heading", { name: "Fixes" })).toBeInTheDocument();
  expect(screen.getByText("Better").tagName).toBe("STRONG");
  expect(document.querySelector("script")).toBeNull();
  expect(screen.getByRole("dialog").querySelector("img")).toBeNull();
  expect(screen.queryByRole("link", { name: "bad" })).toBeNull();
});

it("does not show highlights belonging to another release", async () => {
  vi.mocked(invoke).mockResolvedValue({ version: "2.1.0-beta.7", body: "- Wrong release" });
  render(<WhatsNewDialog version={version} onClose={() => {}} />);
  expect(await screen.findByText(fallback)).toBeInTheDocument();
  expect(screen.queryByText("Wrong release")).toBeNull();
});

it("ignores a stale response after switching versions", async () => {
  let resolve!: (value: { version: string; body: string }) => void;
  vi.mocked(invoke).mockImplementationOnce(() => new Promise((r) => { resolve = r; }));
  const { rerender } = render(<WhatsNewDialog version="2.1.0-beta.5" onClose={() => {}} />);
  vi.mocked(invoke).mockResolvedValue({ version, body: "- Current highlights" });
  rerender(<WhatsNewDialog version={version} onClose={() => {}} />);
  expect(await screen.findByText("Current highlights")).toBeInTheDocument();
  await act(async () => { resolve({ version: "2.1.0-beta.5", body: "- Stale highlights" }); });
  expect(screen.queryByText("Stale highlights")).toBeNull();
  expect(screen.getByRole("link", { name: "See full release notes" })).toHaveAttribute("href", url);
});

it("dismisses the overview", async () => {
  vi.mocked(invoke).mockResolvedValue({ version, body: "- Better dictation" });
  const onClose = vi.fn();
  render(<WhatsNewDialog version={version} onClose={onClose} />);
  await screen.findByText("Better dictation");
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(onClose).toHaveBeenCalledOnce();
});
