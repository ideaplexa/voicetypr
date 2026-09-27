import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";
import { SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { TooltipProvider } from "@/components/ui/tooltip";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("2.0.5"),
}));

vi.mock("@/contexts/LicenseContext", () => ({
  useLicense: () => ({
    status: { status: "licensed", license_type: "pro", trial_days_left: null },
    isLoading: false,
  }),
}));

vi.mock("@/services/updateService", () => ({
  updateService: { checkForUpdatesManually: vi.fn() },
}));

function renderSidebar(activeSection: Parameters<typeof Sidebar>[0]["activeSection"] = "overview") {
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
});

describe("Sidebar navigation", () => {
  it("renders one flat ordered list of destinations", async () => {
    renderSidebar();
    await screen.findByText("v2.0.5");
    expect(document.querySelector('[data-slot="sidebar-container"]')).toHaveClass(
      "group-data-[side=left]:border-r-0",
    );

    const sources = screen.getByRole("button", { name: "Sources" });
    const recording = screen.getByRole("button", { name: "Recording" });
    const polish = screen.getByRole("button", { name: "Polish" });
    expect(
      sources.compareDocumentPosition(recording) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      recording.compareDocumentPosition(polish) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: /network sharing/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "CLI" })).toBeInTheDocument();
  });

  it("keeps everyday destinations compact and support actions fixed below", async () => {
    const user = userEvent.setup();
    const onSectionChange = renderSidebar();
    await screen.findByText("v2.0.5");

    const mainNav = screen.getByTestId("sidebar-main-nav");
    const overview = within(mainNav).getByRole("button", { name: "Overview" });
    const general = within(mainNav).getByRole("button", { name: "General" });
    const history = within(mainNav).getByRole("button", { name: "History" });
    const footerGroup = screen.getByTestId("sidebar-footer-nav");
    const diagnostics = within(footerGroup).getByRole("button", { name: "Quick help" });
    const report = within(footerGroup).getByRole("button", { name: "Report a problem" });

    expect(
      overview.compareDocumentPosition(general) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      general.compareDocumentPosition(history) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(within(mainNav).getByRole("button", { name: "License" })).toBeInTheDocument();
    expect(within(mainNav).queryByRole("button", { name: "Quick help" })).not.toBeInTheDocument();
    expect(
      diagnostics.compareDocumentPosition(report) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(footerGroup).not.toHaveTextContent(/general|license/i);
    expect(document.querySelector('[data-slot="sidebar-content"]')).toHaveClass("overflow-hidden");

    await user.click(general);
    expect(onSectionChange).toHaveBeenLastCalledWith("general");

    await user.click(screen.getByRole("button", { name: /Pro. Open License/i }));
    expect(onSectionChange).toHaveBeenLastCalledWith("license");
  });

  it("keeps the brand row close to the native titlebar", () => {
    renderSidebar();
    expect(document.querySelector('[data-slot="sidebar-header"]')).toHaveClass("pt-1");
  });

  it("collapses to the icon rail through the sidebar control contract", async () => {
    const user = userEvent.setup();
    renderSidebar();
    const sidebar = document.querySelector('[data-slot="sidebar"][data-state]');

    expect(sidebar).toHaveAttribute("data-state", "expanded");
    await user.click(screen.getByRole("button", { name: "Toggle Sidebar" }));
    expect(sidebar).toHaveAttribute("data-state", "collapsed");
    expect(sidebar).toHaveAttribute("data-collapsible", "icon");
    expect(screen.getByTitle("Overview")).toHaveAccessibleName("Overview");
  });
});
