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
      className="group-data-[side=left]:border-r-0 bg-sidebar pt-9"
    >
      <SidebarHeader className="px-3 pb-3 pt-1 group-data-[collapsible=icon]:px-2">
        <button
          type="button"
          onClick={() => onSectionChange("home")}
          aria-label="Voicetypr Home"
          title="Voicetypr Home"
          className="flex w-full items-center gap-2 rounded-[10px] px-2 py-2 text-left hover:bg-sidebar-accent group-data-[collapsible=icon]:justify-center"
        >
          <Brandmark className="size-6 shrink-0 text-sage" />
          <span className="truncate text-sm font-semibold tracking-tight group-data-[collapsible=icon]:hidden">
            Voicetypr
          </span>
        </button>
      </SidebarHeader>
      <SidebarContent className="overflow-hidden px-2">
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
      <SidebarFooter className="gap-1 px-2 pb-3">
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
            "flex w-full items-center gap-2 rounded-[10px] border border-border bg-card px-2 py-2 text-left shadow-sm group-data-[collapsible=icon]:justify-center",
            current === "license" && "ring-1 ring-sage",
          )}
        >
          <licenseScreen.icon className={cn("size-4 shrink-0", license.color)} />
          <span className="min-w-0 truncate text-xs font-semibold group-data-[collapsible=icon]:hidden">
            {license.label}
          </span>
          <span className="ml-auto font-mono text-[11px] text-muted-foreground group-data-[collapsible=icon]:hidden">
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
    <SidebarGroup className="py-1">
      {label ? (
        <SidebarGroupLabel className="px-3 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground group-data-[collapsible=icon]:hidden">
          {label}
        </SidebarGroupLabel>
      ) : null}
      <SidebarGroupContent>
        <SidebarMenu className="group-data-[collapsible=icon]:items-center">
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
          "rounded-[10px] text-[13px] font-medium text-muted-foreground transition-colors [&>svg]:text-muted-foreground",
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
