import { Brandmark } from "@/components/Brandmark";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  footerNavScreens,
  licenseScreen,
  mainNavScreens,
  resolveScreen,
  tuningNavScreens,
  type ScreenDefinition,
  type ScreenId,
} from "@/components/navigation";
import { getVersion } from "@tauri-apps/api/app";
import {
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  useSidebar,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  Sidebar as SidebarPrimitive,
} from "@/components/ui/sidebar";
import { useLicense } from "@/contexts/LicenseContext";
import type { LicenseStatus } from "@/types";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { useEffect, useState } from "react";

const railTooltipClass =
  "rounded-[6px] px-[9px] py-[5px] text-[12px] leading-[normal] font-medium shadow-[0_4px_12px_#00000026] [&>[aria-hidden=true]]:hidden";

interface SidebarProps {
  activeSection: ScreenId;
  onSectionChange: (section: ScreenId) => void;
}

function licenseState(status: LicenseStatus | null, daysLeft: number) {
  if (status?.status === "licensed") return { label: "Pro", color: "text-sage" };
  if (status?.status === "trial") {
    if (daysLeft === 0) return { label: "Trial expires today", color: "text-warn" };
    if (daysLeft === 1) return { label: "Trial · 1 day left", color: "text-warn" };
    return { label: daysLeft > 1 ? `Trial · ${daysLeft} days left` : "Trial", color: "text-sage" };
  }
  return status?.status === "expired"
    ? { label: "Trial expired", color: "text-destructive" }
    : { label: "No License", color: "text-muted-foreground" };
}

export function Sidebar({ activeSection, onSectionChange }: SidebarProps) {
  const { status } = useLicense();
  const { state } = useSidebar();
  const collapsed = state === "collapsed";
  const [version, setVersion] = useState("—");
  const license = licenseState(status, status?.trial_days_left ?? -1);
  const current = resolveScreen(activeSection).screen;
  useEffect(() => {
    void getVersion()
      .then(setVersion)
      .catch(() => setVersion("—"));
  }, []);

  return (
    <SidebarPrimitive
      collapsible="icon"
      data-pencil-name="Sidebar"
      className={cn("bg-sidebar border-r border-border", isMacOS ? "pt-[44px]" : "pt-9")}
    >
      <SidebarHeader
        className={cn("gap-0", collapsed ? "items-center px-0 pb-3 pt-0" : "px-[18px] pb-4 pt-1")}
      >
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                onClick={() => onSectionChange("home")}
                aria-label="Voicetypr Home"
                className={cn(
                  "flex items-center gap-2 rounded-[8px] p-0 text-left hover:bg-sidebar-accent",
                  collapsed ? "size-9 justify-center" : "w-full",
                )}
              />
            }
          >
            <Brandmark className="size-[22px] shrink-0 text-sage" />
            <span
              className={collapsed ? "sr-only" : "truncate text-sm leading-[normal] font-semibold"}
            >
              Voicetypr
            </span>
          </TooltipTrigger>
          <TooltipContent
            role="tooltip"
            side="right"
            sideOffset={8}
            hidden={!collapsed}
            className={railTooltipClass}
          >
            Voicetypr Home
          </TooltipContent>
        </Tooltip>
      </SidebarHeader>
      <SidebarContent className={cn("gap-0 overflow-y-auto", collapsed ? "px-0" : "px-[10px]")}>
        <nav aria-label="Main navigation">
          <NavGroup
            items={mainNavScreens}
            activeSection={current}
            onSectionChange={onSectionChange}
          />
          <div
            data-pencil-name="Divider"
            className={cn(
              "mx-auto",
              collapsed ? "w-10 px-2 py-[9px]" : "w-full px-[10px] py-[11px]",
            )}
          >
            <div role="separator" className="h-px bg-border" />
          </div>
          <NavGroup
            items={tuningNavScreens}
            activeSection={current}
            onSectionChange={onSectionChange}
          />
        </nav>
      </SidebarContent>
      <SidebarFooter className={cn("gap-0.5 pb-3", collapsed ? "items-center px-0" : "px-[10px]")}>
        <nav aria-label="Support navigation">
          <NavGroup
            items={footerNavScreens}
            activeSection={current}
            onSectionChange={onSectionChange}
          />
        </nav>
        <Tooltip>
          <TooltipTrigger
            render={
              <button
                type="button"
                onClick={() => onSectionChange("license")}
                aria-label={`${license.label}. Open License`}
                aria-current={current === "license" ? "page" : undefined}
                className={cn(
                  "flex items-center gap-2 rounded-[8px] bg-card text-left ring-1 ring-inset ring-border",
                  collapsed ? "h-[34px] w-10 justify-center p-0" : "w-full px-[10px] py-2",
                  current === "license" && "ring-1 ring-sage",
                )}
              />
            }
          >
            <licenseScreen.icon className={cn("size-4 shrink-0", license.color)} />
            <span
              className={
                collapsed
                  ? "sr-only"
                  : "min-w-0 truncate text-[12.5px] leading-[normal] font-semibold"
              }
            >
              {license.label}
            </span>
            <span className={collapsed ? "sr-only" : "text-[11.5px] leading-[normal] text-text-3"}>
              {version}
            </span>
          </TooltipTrigger>
          <TooltipContent
            role="tooltip"
            side="right"
            sideOffset={8}
            hidden={!collapsed}
            className={railTooltipClass}
          >
            {license.label} · License
          </TooltipContent>
        </Tooltip>
      </SidebarFooter>
    </SidebarPrimitive>
  );
}

function NavGroup({
  items,
  activeSection,
  onSectionChange,
}: {
  items: ScreenDefinition[];
  activeSection: string;
  onSectionChange: (section: ScreenId) => void;
}) {
  return (
    <SidebarGroup className="p-0">
      <SidebarGroupContent>
        <SidebarMenu className="gap-0.5 group-data-[collapsible=icon]:items-center">
          {items.map((item) => (
            <SidebarNavItem
              key={item.id}
              item={item}
              active={activeSection === item.id}
              onSelect={onSectionChange}
            />
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

function SidebarNavItem({
  item,
  active,
  onSelect,
}: {
  item: ScreenDefinition;
  active: boolean;
  onSelect: (section: ScreenId) => void;
}) {
  const Icon = item.icon;
  const { state } = useSidebar();
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        size="sm"
        tooltip={{
          role: "tooltip",
          children: item.label,
          sideOffset: 8,
          className: railTooltipClass,
        }}
        aria-label={item.label}
        isActive={active}
        aria-current={active ? "page" : undefined}
        onClick={() => onSelect(item.id)}
        className={cn(
          "group-data-[collapsible=icon]:size-[40px]! group-data-[collapsible=icon]:h-[34px]! group-data-[collapsible=icon]:p-0! group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:gap-0 h-auto gap-[10px] rounded-[8px] px-[10px] py-[7px] text-[13px] leading-[normal] font-medium [&>svg]:size-4 text-muted-foreground transition-colors [&>svg]:text-muted-foreground",
          active
            ? "data-active:bg-card data-active:font-semibold data-active:text-foreground data-active:shadow-[0_1px_2px_#0000000f] data-active:hover:bg-card data-active:[&>svg]:text-sage"
            : "hover:bg-sidebar-accent hover:text-foreground",
        )}
      >
        <Icon />
        <span className={state === "collapsed" ? "sr-only" : undefined}>{item.label}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
