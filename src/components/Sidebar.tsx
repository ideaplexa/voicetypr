import { Brandmark } from "@/components/Brandmark";
import {
  footerNavScreens,
  navScreens,
  type ScreenDefinition,
  type ScreenId,
} from "@/components/navigation";
import { getVersion } from "@tauri-apps/api/app";
import { Button } from "@/components/ui/button";
import {
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  Sidebar as SidebarPrimitive,
} from "@/components/ui/sidebar";
import { useLicense } from "@/contexts/LicenseContext";
import type { LicenseStatus } from "@/types";
import { cn } from "@/lib/utils";
import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { updateService } from "@/services/updateService";

interface SidebarProps {
  activeSection: ScreenId;
  onSectionChange: (section: ScreenId) => void;
}

function getLicenseBadge(status: LicenseStatus | null, daysLeft: number) {
  if (!status || status.status === "none") {
    return {
      label: "No License",
      className: "border-border bg-muted text-muted-foreground",
    };
  }

  if (status.status === "licensed") {
    return {
      label: "Pro",
      className: "border-emerald-500/30 bg-emerald-500/10 text-emerald-800",
    };
  }

  if (status.status === "trial") {
    if (daysLeft > 1) {
      return {
        label: `Trial · ${daysLeft} days left`,
        className: "border-green-500/25 bg-green-500/10 text-green-800 dark:text-green-400",
      };
    }

    if (daysLeft === 1) {
      return {
        label: "Trial · 1 day left",
        className: "border-amber-500/25 bg-amber-500/10 text-amber-800 dark:text-amber-400",
      };
    }

    if (daysLeft === 0) {
      return {
        label: "Trial expires today",
        className: "border-amber-500/25 bg-amber-500/10 text-amber-800 dark:text-amber-400",
      };
    }

    return {
      label: "Trial",
      className: "border-green-500/25 bg-green-500/10 text-green-800 dark:text-green-400",
    };
  }

  return {
    label: "Trial expired",
    className: "border-red-500/25 bg-red-500/10 text-red-800 dark:text-red-400",
  };
}
export function Sidebar({ activeSection, onSectionChange }: SidebarProps) {
  const { status, isLoading } = useLicense();
  const [appVersion, setAppVersion] = useState("—");
  const licenseBadge = getLicenseBadge(status, status?.trial_days_left ?? -1);
  const licenseNeedsAttention =
    status?.status === "expired" || status?.verification_state === "needs_revalidation";

  useEffect(() => {
    const loadVersion = async () => {
      try {
        setAppVersion(await getVersion());
      } catch {
        setAppVersion("—");
      }
    };
    void loadVersion();
  }, []);

  return (
    <SidebarPrimitive
      collapsible="icon"
      className="group-data-[side=left]:border-r-0 bg-sidebar/95 pt-9 backdrop-blur-sm"
    >
      <SidebarHeader className="gap-2 px-4 pb-2 pt-1 group-data-[collapsible=icon]:px-2">
        <div className="flex w-full items-center gap-2 rounded-lg px-1">
          <button
            type="button"
            onClick={() => onSectionChange("overview")}
            aria-label="Overview"
            title="Overview"
            className="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg px-2 py-2 text-left transition-colors hover:bg-sidebar-accent group-data-[collapsible=icon]:justify-center"
          >
            <Brandmark className="size-6 shrink-0 text-sage" />
            <span className="truncate text-sm font-semibold tracking-tight group-data-[collapsible=icon]:hidden">
              Voicetypr
            </span>
          </button>
          {!isLoading && status ? (
            <button
              type="button"
              onClick={() => onSectionChange("license")}
              aria-label={`${licenseBadge.label}. Open License`}
              title="Open License"
              className={cn(
                "shrink-0 rounded-full border px-2 py-0.5 text-[11px] font-semibold uppercase tracking-[0.12em] transition-shadow hover:ring-2 hover:ring-sage/20 group-data-[collapsible=icon]:hidden",
                licenseBadge.className,
                licenseNeedsAttention && "ring-2 ring-amber-500/25",
              )}
            >
              {licenseBadge.label}
            </button>
          ) : null}
        </div>
      </SidebarHeader>

      <SidebarContent className="overflow-hidden px-2">
        <SidebarNavMenu
          items={navScreens}
          activeSection={activeSection}
          onSectionChange={onSectionChange}
        />
      </SidebarContent>

      <SidebarFooter className="gap-1 px-2">
        <nav data-testid="sidebar-footer-nav">
          <SidebarGroup className="py-1">
            <SidebarGroupContent>
              <SidebarMenu className="group-data-[collapsible=icon]:items-center">
                {footerNavScreens.map((item) => (
                  <SidebarNavItem
                    key={item.id}
                    item={item}
                    isActive={activeSection === item.id}
                    onSelect={onSectionChange}
                  />
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </nav>
        <SidebarFooterStatus appVersion={appVersion} />
      </SidebarFooter>
    </SidebarPrimitive>
  );
}

function SidebarNavMenu({
  items,
  activeSection,
  onSectionChange,
}: {
  items: ScreenDefinition[];
  activeSection: ScreenId;
  onSectionChange: (section: ScreenId) => void;
}) {
  return (
    <nav data-testid="sidebar-main-nav">
      <SidebarGroup className="py-1">
        <SidebarGroupContent>
          <SidebarMenu className="group-data-[collapsible=icon]:items-center">
            {items.map((item) => (
              <SidebarNavItem
                key={item.id}
                item={item}
                isActive={activeSection === item.id}
                onSelect={onSectionChange}
              />
            ))}
          </SidebarMenu>
        </SidebarGroupContent>
      </SidebarGroup>
    </nav>
  );
}

function SidebarNavItem({
  item,
  isActive,
  onSelect,
}: {
  item: ScreenDefinition;
  isActive: boolean;
  onSelect: (section: ScreenId) => void;
}) {
  const Icon = item.icon;

  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        size="sm"
        tooltip={item.description}
        isActive={isActive}
        onClick={() => onSelect(item.id)}
        className={cn(
          "rounded-xl text-sm font-medium transition-colors [&>svg]:text-muted-foreground",
          isActive
            ? "!bg-card font-semibold text-foreground shadow-sm ring-1 ring-border hover:!bg-card [&>svg]:!text-sage"
            : "text-muted-foreground hover:bg-sidebar-accent/70 hover:text-foreground",
        )}
      >
        <Icon />
        <span>{item.label}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}

function SidebarFooterStatus({ appVersion }: { appVersion: string }) {
  const [isCheckingUpdates, setIsCheckingUpdates] = useState(false);

  const checkUpdates = async () => {
    setIsCheckingUpdates(true);
    try {
      await updateService.checkForUpdatesManually();
    } finally {
      setIsCheckingUpdates(false);
    }
  };

  return (
    <div className="flex flex-col gap-2 px-2">
      <div className="flex items-center justify-between gap-2 group-data-[collapsible=icon]:justify-center">
        <span className="text-xs text-muted-foreground group-data-[collapsible=icon]:hidden">
          v{appVersion}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="size-7 rounded-md text-muted-foreground"
          onClick={checkUpdates}
          disabled={isCheckingUpdates}
          title="Check for updates"
        >
          <RefreshCw className={cn("size-3.5", isCheckingUpdates && "animate-spin")} />
          <span className="sr-only">Check for updates</span>
        </Button>
      </div>
    </div>
  );
}
