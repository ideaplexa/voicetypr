import type { ComponentProps, ComponentType, ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { CircleHelp } from "lucide-react";

interface IconProps {
  className?: string;
}

/** Shared page, card, and control patterns for the 2.1 settings screens. */

export function SettingsPage({
  children,
  className,
  wide = false,
  container = false,
}: {
  children: ReactNode;
  className?: string;
  wide?: boolean;
  container?: boolean;
}) {
  return (
    <div className={cn("h-full min-h-0 overflow-auto", container && "@container")}>
      <div
        className={cn(
          "mx-auto flex w-full flex-col gap-5 pb-4 pl-2 pr-4",
          wide ? "max-w-5xl" : "max-w-3xl",
          className,
        )}
      >
        {children}
      </div>
    </div>
  );
}

export function PageHeader({
  title,
  description,
  action,
}: {
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <header className="flex flex-wrap items-start gap-4">
      <div className="min-w-0">
        <h1 className="text-[24px] font-semibold tracking-tight text-foreground">{title}</h1>
        {description ? (
          <p className="mt-1 max-w-2xl text-sm text-muted-foreground">{description}</p>
        ) : null}
      </div>
      {action ? <div className="ml-auto flex flex-wrap items-center gap-2">{action}</div> : null}
    </header>
  );
}

export function SettingsHeader({
  actions,
  ...props
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return <PageHeader {...props} action={actions} />;
}

/** Compact card and row geometry used by the Settings inner panes. */
export function SettingsPaneHeader({ title, description }: { title: string; description: string }) {
  return (
    <header className="space-y-[3px]">
      <h2 className="text-[15px] font-semibold text-foreground">{title}</h2>
      <p className="text-[12.5px] text-muted-foreground">{description}</p>
    </header>
  );
}

export function SettingsPaneCard({
  title,
  children,
  className,
}: {
  title?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("rounded-[14px] border border-border bg-card px-5 py-1", className)}>
      {title ? (
        <h3 className="pt-[14px] pb-1 text-sm font-semibold text-foreground">{title}</h3>
      ) : null}
      <div>{children}</div>
    </section>
  );
}

export function SettingsPaneRow({
  title,
  description,
  control,
  className,
}: {
  title: ReactNode;
  description?: ReactNode;
  control?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex min-h-[60px] items-center gap-5 border-t border-border py-3 first:border-t-0",
        className,
      )}
    >
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium text-foreground">{title}</p>
        {description ? <p className="mt-0.5 text-xs text-muted-foreground">{description}</p> : null}
      </div>
      {control ? <div className="shrink-0">{control}</div> : null}
    </div>
  );
}

export function InfoButton({
  label,
  ...props
}: { label: string } & Omit<ComponentProps<typeof Button>, "children">) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={label}
      title={label}
      {...props}
    >
      <CircleHelp className="size-4" />
    </Button>
  );
}

export function SettingsCard({
  compact = false,
  icon: Icon,
  title,
  description,
  action,
  children,
  className,
}: {
  compact?: boolean;
  icon?: ComponentType<IconProps>;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <section
      className={cn(
        "rounded-[14px] border border-border bg-card",
        compact ? "px-5 py-1" : "p-5",
        className,
      )}
    >
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2.5">
            {Icon ? <Icon className="h-4 w-4 shrink-0 text-sage" /> : null}
            <h2
              className={cn(
                "font-semibold text-foreground",
                compact ? "pt-[14px] pb-1 text-sm" : "text-base",
              )}
            >
              {title}
            </h2>
          </div>
          {description ? (
            <p
              className={cn(
                "mt-1 text-sm leading-relaxed text-muted-foreground",
                Icon && "ml-[26px]",
              )}
            >
              {description}
            </p>
          ) : null}
        </div>
        {action ? <div className="shrink-0">{action}</div> : null}
      </div>
      {children ? <div className={compact ? "" : "mt-1"}>{children}</div> : null}
    </section>
  );
}

export function SettingRow({
  title,
  description,
  htmlFor,
  control,
  children,
  className,
}: {
  title: ReactNode;
  description?: ReactNode;
  htmlFor?: string;
  control?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-start gap-3 border-t border-border pt-4 mt-4 first:mt-3 first:border-t-0 first:pt-0 sm:flex-row sm:items-center sm:gap-6",
        className,
      )}
    >
      <div className="min-w-0 max-w-[440px]">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="block text-sm font-medium text-foreground">
            {title}
          </label>
        ) : (
          <p className="text-sm font-medium text-foreground">{title}</p>
        )}
        {description ? (
          <p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">{description}</p>
        ) : null}
      </div>
      <div className="flex w-full items-center gap-2 sm:ml-auto sm:w-auto sm:shrink-0">
        {control ?? children}
      </div>
    </div>
  );
}

export interface ChoiceOption {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
}

export function Segmented({
  label,
  value,
  onValueChange,
  options,
}: {
  label: string;
  value: string;
  onValueChange: (value: string) => void;
  options: ChoiceOption[];
}) {
  return (
    <ToggleGroup
      aria-label={label}
      value={[value]}
      onValueChange={(values) => {
        const next = values.find((candidate) => candidate !== value);
        if (next) onValueChange(next);
      }}
      className="rounded-[7px] bg-muted p-0.5"
      spacing={0.5}
    >
      {options.map((option) => (
        <ToggleGroupItem
          key={option.value}
          value={option.value}
          disabled={option.disabled}
          className="rounded-[7px] border border-transparent bg-transparent px-3 font-medium text-muted-foreground aria-pressed:bg-card! aria-pressed:text-foreground aria-pressed:font-semibold aria-pressed:shadow-sm dark:aria-pressed:border-foreground/30"
        >
          {option.label}
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  );
}

export function ChoiceCard({
  label,
  description,
  icon: Icon,
  tag,
  selected,
  active,
  onSelect,
}: {
  label: string;
  description?: string;
  icon?: ComponentType<IconProps>;
  tag?: string;
  selected: boolean;
  active?: boolean;
  onSelect: () => void;
}) {
  return (
    <Button
      type="button"
      variant="outline"
      aria-label={label}
      aria-pressed={selected}
      onClick={onSelect}
      className={cn(
        "h-full min-h-36 w-full flex-col items-start justify-start gap-2 rounded-[14px] border bg-card p-4 text-left whitespace-normal",
        (active ?? selected) && "border-sage ring-[1.5px] ring-sage",
        active === false && selected && "ring-[1.5px] ring-foreground/20",
      )}
    >
      {Icon ? (
        <span className="rounded-lg bg-muted p-2 text-sage">
          <Icon className="size-4" />
        </span>
      ) : null}
      <span className="font-medium text-foreground">{label}</span>
      {description ? (
        <span className="text-xs leading-snug text-muted-foreground">{description}</span>
      ) : null}
      {tag ? (
        <span className="mt-auto rounded-md bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
          {tag}
        </span>
      ) : null}
    </Button>
  );
}
