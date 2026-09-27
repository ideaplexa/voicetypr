import type { ComponentType, ReactNode } from "react";
import { cn } from "@/lib/utils";

interface IconProps {
  className?: string;
}

/**
 * Shared settings layout primitives — the Claude dashboard "settings card" pattern.
 * Every Configure/Account screen (Dictation, Transcription, Formatting, License,
 * Diagnostics…) composes these so the whole app reads as one system: a page header,
 * a stack of titled cards, and label/description + control rows inside each card.
 */

export function SettingsPage({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div className="h-full min-h-0 overflow-auto">
      <div className={cn("mx-auto flex w-full max-w-4xl flex-col gap-5 pb-4 pl-2 pr-4", className)}>
        {children}
      </div>
    </div>
  );
}

export function SettingsHeader({
  title,
  description,
  actions,
}: {
  title: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="flex flex-wrap items-start gap-4">
      <div className="min-w-0">
        <h1 className="text-[1.75rem] font-semibold tracking-[-0.025em] text-foreground">
          {title}
        </h1>
        {description ? (
          <p className="mt-1 max-w-2xl text-sm leading-relaxed text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      {actions ? <div className="ml-auto flex flex-wrap items-center gap-2">{actions}</div> : null}
    </header>
  );
}

export function SettingsCard({
  icon: Icon,
  title,
  description,
  action,
  children,
  className,
}: {
  icon?: ComponentType<IconProps>;
  title: ReactNode;
  description?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("rounded-xl border border-border/80 bg-card p-5", className)}>
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2.5">
            {Icon ? <Icon className="h-4 w-4 shrink-0 text-sage" /> : null}
            <h2 className="text-base font-semibold text-foreground">{title}</h2>
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
      {children ? <div className="mt-1">{children}</div> : null}
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
        "flex flex-col items-start gap-3 border-t border-border/70 pt-4 mt-4 first:mt-3 first:border-t-0 first:pt-0 sm:flex-row sm:items-center sm:gap-6",
        className,
      )}
    >
      <div className="min-w-0 max-w-[440px]">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="block text-[13.5px] font-semibold text-foreground">
            {title}
          </label>
        ) : (
          <p className="text-[13.5px] font-semibold text-foreground">{title}</p>
        )}
        {description ? (
          <p className="mt-0.5 text-[12.5px] leading-relaxed text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      <div className="flex w-full items-center gap-2 sm:ml-auto sm:w-auto sm:shrink-0">
        {control ?? children}
      </div>
    </div>
  );
}
