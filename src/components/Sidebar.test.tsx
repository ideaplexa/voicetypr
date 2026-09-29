import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";
import { SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";

const licenseState = vi.hoisted(() => ({
  current: { status: "licensed", license_type: "pro", trial_days_left: null as number | null },
}));

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn().mockResolvedValue("2.1.0") }));
vi.mock("@/contexts/LicenseContext", () => ({
  useLicense: () => ({
    status: licenseState.current,
    isLoading: false,
  }),
}));

function renderSidebar(activeSection: Parameters<typeof Sidebar>[0]["activeSection"] = "home") {
  const onSectionChange = vi.fn();
  render(
    <TooltipProvider>
      <SidebarProvider>
        <SidebarTrigger />
        <Sidebar activeSection={activeSection} onSectionChange={onSectionChange} />
      </SidebarProvider>
    </TooltipProvider>,
  );
  return onSectionChange;
}

beforeEach(() => {
  vi.clearAllMocks();
  licenseState.current = { status: "licensed", license_type: "pro", trial_days_left: null };
});

describe("Sidebar navigation", () => {
  it("shows the new ordered destinations and Setup group", async () => {
    renderSidebar();
    const main = within(screen.getByRole("navigation", { name: "Main navigation" }));
    expect(main.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Home",
      "History",
      "Transcription",
      "Polish",
      "Recording",
    ]);
    expect(main.getByText("Setup")).toHaveClass("text-muted-foreground");
    const support = within(screen.getByRole("navigation", { name: "Support navigation" }));
    expect(support.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Settings",
      "Help & feedback",
    ]);
    expect(main.queryByRole("button", { name: "Upload" })).not.toBeInTheDocument();
    expect(await screen.findByText("2.1.0")).toHaveClass("text-muted-foreground");
    expect(screen.queryByRole("button", { name: "Check for updates" })).not.toBeInTheDocument();
  });

  it("marks the current destination and navigates through brand, footer and license", async () => {
    const user = userEvent.setup();
    const change = renderSidebar("transcription");
    expect(screen.getByRole("button", { name: "Transcription" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    await user.click(screen.getByRole("button", { name: "Voicetypr Home" }));
    await user.click(screen.getByRole("button", { name: "Settings" }));
    await user.click(screen.getByRole("button", { name: /Pro. Open License/ }));
    expect(change.mock.calls.map(([section]) => section)).toEqual(["home", "settings", "license"]);
  });

  it("shows legacy destinations as their current sidebar destination", () => {
    renderSidebar("advanced");
    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("names trial and expired license states in the bottom chip", () => {
    licenseState.current = { status: "trial", license_type: "pro", trial_days_left: 1 };
    const view = renderSidebar("license");
    expect(
      screen.getByRole("button", { name: /Trial · 1 day left. Open License/ }),
    ).toHaveAttribute("aria-current", "page");
    view.mockClear();
    licenseState.current = { status: "expired", license_type: "pro", trial_days_left: null };
    // A fresh render reflects a backend license state change.
    renderSidebar();
    expect(screen.getByRole("button", { name: /Trial expired. Open License/ })).toBeInTheDocument();
  });

  it("collapses to an icon rail while keeping navigation accessible", async () => {
    const user = userEvent.setup();
    renderSidebar();
    expect(document.querySelector('[data-slot="sidebar"]')).toHaveAttribute("data-state", "expanded");
    await user.click(screen.getByRole("button", { name: "Toggle Sidebar" }));
    expect(document.querySelector('[data-slot="sidebar"]')).toHaveAttribute("data-state", "collapsed");
    expect(screen.getByRole("button", { name: "Home" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Help & feedback" })).toBeInTheDocument();
  });
});
