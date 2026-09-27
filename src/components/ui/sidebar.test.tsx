import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Sidebar, SidebarContent, SidebarFooter, SidebarInset, SidebarProvider } from "./sidebar";

describe("sidebar layout", () => {
  it("pins the fixed desktop layout to the viewport instead of page-scrolling", () => {
    render(
      <SidebarProvider>
        <Sidebar collapsible="none">
          <SidebarContent>
            <div>Sidebar content</div>
          </SidebarContent>
          <SidebarFooter>Footer stays pinned</SidebarFooter>
        </Sidebar>
        <SidebarInset>
          <div style={{ height: 4000 }}>Overflowing main content</div>
        </SidebarInset>
      </SidebarProvider>,
    );

    const sidebar = screen.getByText("Sidebar content").closest('[data-slot="sidebar"]');
    const sidebarContent = screen
      .getByText("Sidebar content")
      .closest('[data-slot="sidebar-content"]');
    const sidebarFooter = screen
      .getByText("Footer stays pinned")
      .closest('[data-slot="sidebar-footer"]');
    const inset = screen
      .getByText("Overflowing main content")
      .closest('[data-slot="sidebar-inset"]');

    expect(sidebar).toHaveClass("h-full");
    expect(sidebarContent).toHaveClass("flex-1", "overflow-auto");
    expect(sidebarFooter).not.toBeNull();
    expect(sidebarFooter?.parentElement).toBe(sidebar);
    expect(sidebarContent?.parentElement).toBe(sidebar);
    expect(inset).toHaveClass("flex-1");
  });
});
