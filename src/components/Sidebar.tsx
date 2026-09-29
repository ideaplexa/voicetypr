import { Brandmark } from "@/components/Brandmark";
import {
  footerNavScreens,
  licenseScreen,
  mainNavScreens,
  resolveScreen,
  setupNavScreens,
  type ScreenDefinition,
  type ScreenId,
} from "@/components/navigation";
import { getVersion } from "@tauri-apps/api/app";
import {
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
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
      className={cn("bg-sidebar border-r border-border", isMacOS ? "pt-[44px]" : "pt-4")}
    >
      <SidebarHeader className="px-[10px] pb-[14px] pt-1 group-data-[collapsible=icon]:px-2">
        <button
          type="button"
          onClick={() => onSectionChange("home")}
          aria-label="Voicetypr Home"
          title="Voicetypr Home"
          className="flex w-full items-center gap-2 rounded-[8px] px-2 py-0 text-left hover:bg-sidebar-accent group-data-[collapsible=icon]:justify-center"
        >
          <Brandmark className="size-[22px] shrink-0 text-sage" />
          <span className="truncate text-sm font-semibold tracking-tight group-data-[collapsible=icon]:hidden">
            Voicetypr
          </span>
        </button>
      </SidebarHeader>
      <SidebarContent className="overflow-hidden px-[10px]">
        <nav aria-label="Main navigation">
          <NavGroup
            items={mainNavScreens}
            activeSection={current}
            onSectionChange={onSectionChange}
          />
          <NavGroup
            label="Setup"
            items={setupNavScreens}
            activeSection={current}
            onSectionChange={onSectionChange}
          />
        </nav>
      </SidebarContent>
      <SidebarFooter className="gap-0.5 px-[10px] pb-3">
        <nav aria-label="Support navigation">
          <NavGroup
            items={footerNavScreens}
            activeSection={current}
            onSectionChange={onSectionChange}
          />
        </nav>
        <button
          type="button"
          onClick={() => onSectionChange("license")}
          aria-label={`${license.label}. Open License`}
          aria-current={current === "license" ? "page" : undefined}
          title="Open License"
          className={cn(
            "flex w-full items-center gap-2 rounded-[8px] border border-border bg-card px-[10px] py-2 text-left shadow-sm group-data-[collapsible=icon]:justify-center",
            current === "license" && "ring-1 ring-sage",
          )}
        >
          <licenseScreen.icon className={cn("size-4 shrink-0", license.color)} />
          <span className="min-w-0 truncate text-[12.5px] leading-[normal] font-semibold group-data-[collapsible=icon]:hidden">
            {license.label}
          </span>
          <span className="ml-auto text-[11.5px] text-text-3 group-data-[collapsible=icon]:hidden">
            {version}
          </span>
        </button>
      </SidebarFooter>
    </SidebarPrimitive>
  );
}

function NavGroup({
  items,
  label,
  activeSection,
  onSectionChange,
}: {
  items: ScreenDefinition[];
  label?: string;
  activeSection: string;
  onSectionChange: (section: ScreenId) => void;
}) {
  return (
    <SidebarGroup className="p-0">
      {label ? (
        <SidebarGroupLabel className="h-auto px-[10px] pt-[14px] pb-1 text-[10.5px] leading-[normal] font-semibold uppercase tracking-[0.6px] text-text-3 group-data-[collapsible=icon]:hidden">
          {label}
        </SidebarGroupLabel>
      ) : null}
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
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        size="sm"
        tooltip={item.description}
        isActive={active}
        aria-current={active ? "page" : undefined}
        onClick={() => onSelect(item.id)}
        className={cn(
          "h-auto gap-[10px] rounded-[8px] px-[10px] py-[7px] text-[13px] leading-[normal] font-medium [&>svg]:size-4 text-muted-foreground transition-colors [&>svg]:text-muted-foreground",
          active
            ? "bg-card font-semibold text-foreground shadow-sm hover:bg-card [&>svg]:text-sage"
            : "hover:bg-sidebar-accent hover:text-foreground",
        )}
      >
        <Icon />
        <span>{item.label}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
