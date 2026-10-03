import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WhatsNewDialog } from "@/components/WhatsNewDialog";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());
it("offers the installed-version release page when offline", async () => {
  vi.mocked(invoke).mockImplementation(async command => { if (command === "get_release_notes") throw new Error("offline"); return undefined; });
  render(<WhatsNewDialog version="2.1.0-beta.4" onClose={() => {}} />);
  expect(await screen.findByText(/Release notes aren’t available/)).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Open release page" })).toHaveAttribute("href", "https://github.com/ideaplexa/voicetypr/releases/tag/v2.1.0-beta.4");
});
it("renders markdown without raw HTML, remote images or unsafe links", async () => {
  vi.mocked(invoke).mockResolvedValue({version:"2.1.0-beta.4",body:"## Fixes\n\n**Better** dictation\n\n<script>alert(1)</script>\n\n[bad](javascript:alert%281%29)\n\n![tracking](https://example.com/pixel)",url:""});
  render(<WhatsNewDialog version="2.1.0-beta.4" onClose={() => {}} />);
  expect(await screen.findByRole("heading", { name: "Fixes" })).toBeInTheDocument();
  expect(screen.getByText("Better").tagName).toBe("STRONG");
  expect(document.querySelector('script')).toBeNull();
  expect(screen.getByRole("dialog").querySelector('img')).toBeNull();
  expect(screen.queryByRole("link", {name:"bad"})).toBeNull();
});
