import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

/**
 * The round-3c onboarding frame (renders/VOIC-4/round-3): a 160px progress
 * bar, an optional download chip, the screen's content centred, Back at the
 * bottom left and an optional quiet action at the bottom right.
 */
export function Frame({
  progress,
  chip,
  onBack,
  backDisabled,
  right,
  footer,
  children,
}: {
  /** Steps done and the total; omitted on screens outside the count. */
  progress?: { done: number; total: number } | null;
  chip?: ReactNode;
  onBack?: () => void;
  backDisabled?: boolean;
  right?: ReactNode;
  /** A quiet line across the foot of the window, such as Done's crash-report choice. */
  footer?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="relative flex min-h-screen w-full flex-col items-center justify-center bg-background px-6 py-16 text-foreground">
      {progress ? (
        <div
          role="progressbar"
          aria-label="Setup progress"
          aria-valuenow={progress.done}
          aria-valuemin={0}
          aria-valuemax={progress.total}
          className="absolute top-[23px] left-1/2 h-1 w-40 -translate-x-1/2 rounded-[2px] bg-border"
        >
          <div className="h-1 rounded-[2px] bg-sage" style={{ width: `${Math.round((160 * progress.done) / progress.total)}px` }} />
        </div>
      ) : null}
      {chip ? <div className="absolute top-[14px] right-6 flex max-w-[286px] justify-end">{chip}</div> : null}
      {children}
      {onBack ? (
        <button
          type="button"
          disabled={backDisabled}
          onClick={onBack}
          className="absolute bottom-[21px] left-[22px] flex items-center gap-1.5 rounded-md px-2 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground disabled:opacity-50"
        >
          <ChevronLeft className="size-[15px]" />
          Back
        </button>
      ) : null}
      {right ? <div className="absolute right-[22px] bottom-[21px]">{right}</div> : null}
      {footer ? <div className="pointer-events-none absolute inset-x-0 bottom-[33px] flex justify-center *:pointer-events-auto">{footer}</div> : null}
    </div>
  );
}

/** The quiet chip at the top right: the engine download or that it is ready. */
export function FrameChip({ icon, children }: { icon?: ReactNode; children: ReactNode }) {
  return (
    <span className="flex items-center gap-[7px] rounded-full bg-secondary px-2.5 py-[5px] text-[12.5px] leading-[1.2] font-medium text-muted-foreground">
      {icon}
      {children}
    </span>
  );
}

export function FrameHeading({ title, line, size = 34 }: { title: ReactNode; line?: ReactNode; size?: 34 | 36 }) {
  return (
    <div className="flex flex-col items-center gap-3 self-stretch">
      <h2
        className={cn(
          "text-center leading-[1.1] font-semibold whitespace-pre text-foreground",
          size === 36 ? "text-[36px] tracking-[-0.9px]" : "text-[34px] tracking-[-0.8px]",
        )}
      >
        {title}
      </h2>
      {line ? <p className="self-stretch text-center text-base leading-[1.45] text-muted-foreground">{line}</p> : null}
    </div>
  );
}

type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { size?: "md" | "lg" };

export function PrimaryButton({ className, size = "md", type = "button", ...props }: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex items-center justify-center gap-2 bg-sage font-semibold text-sage-foreground transition-opacity hover:opacity-90 disabled:opacity-50",
        size === "lg" ? "h-[54px] rounded-xl text-[17px]" : "h-11 rounded-[10px] px-[22px] text-[15px]",
        className,
      )}
      {...props}
    />
  );
}

export function SecondaryButton({ className, type = "button", ...props }: ButtonProps) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex h-11 items-center justify-center gap-2 rounded-[10px] border border-border bg-card px-[22px] text-[15px] font-semibold text-foreground hover:bg-accent disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

/** Quiet text actions: "I have a key", "Skip Accessibility, I'll paste myself". */
export function QuietButton({ className, type = "button", ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type={type}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md px-1.5 py-1 text-sm font-medium text-muted-foreground hover:text-foreground disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}

/** "Skip the tour ›" and similar quiet links at the bottom right. */
export function QuietNext({ children, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <QuietButton {...props}>
      {children}
      <ChevronRight className="size-[15px]" />
    </QuietButton>
  );
}

export function Radio({ on }: { on: boolean }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "size-5 shrink-0 rounded-full bg-card",
        on ? "border-[3px] border-sage shadow-[0_0_0_3px_var(--sage)]" : "border border-text-3",
      )}
    />
  );
}

/** A rounded white list with hairline rows, as on the language and model screens. */
export function FrameList({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <div className={cn("flex flex-col self-stretch overflow-hidden rounded-[14px] border border-border bg-card", className)}>
      {children}
    </div>
  );
}

export function Tag({ tone = "sage", children }: { tone?: "sage" | "plain"; children: ReactNode }) {
  return (
    <span
      className={cn(
        "rounded-full px-[9px] py-[3px] text-[12.5px] leading-[1.2] font-semibold whitespace-nowrap",
        tone === "sage" ? "bg-sage-bg text-sage" : "bg-secondary text-muted-foreground",
      )}
    >
      {children}
    </span>
  );
}

const NAMES: Record<string, string> = { "⌥": "⌥ Option", "⌘": "⌘ Command", "⌃": "⌃ Control", "⇧": "⇧ Shift" };

/** A shortcut key as drawn on the frames: white, hairline, a heavier bottom edge; Mac modifiers carry their name. */
export function FrameKeys({ caps, size = "md" }: { caps: string[]; size?: "sm" | "md" | "xl" }) {
  return (
    <span className={cn("flex items-center", size === "xl" ? "gap-2.5" : "gap-[5px]")} aria-label={caps.join(" + ")}>
      {caps.map((cap, index) => (
        <span key={`${cap}-${index}`} className={cn("flex items-center", size === "xl" ? "gap-2.5" : "gap-[5px]")}>
          {index > 0 ? <span aria-hidden="true" className={cn("text-text-3", size === "xl" ? "text-[22px]" : "text-[13px]")}>+</span> : null}
          <kbd
            className={cn(
              "flex items-center justify-center border border-b-2 border-border bg-card font-sans font-semibold text-foreground",
              size === "xl" && "h-16 rounded-[14px] border-b-4 px-[22px] text-[24px]",
              size === "md" && "h-[30px] rounded-[7px] px-2.5 text-[13.5px]",
              size === "sm" && "h-[22px] rounded-[5px] px-1.5 text-[11.5px]",
            )}
          >
            {NAMES[cap] ?? cap}
          </kbd>
        </span>
      ))}
    </span>
  );
}

export const machine = isMacOS ? "Mac" : "PC";
