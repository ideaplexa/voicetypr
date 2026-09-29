import { Button } from "@/components/settings/SettingsButton";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/utils";
import { CheckCircle2, HardDrive, Server, Star, Zap } from "lucide-react";
import type { ReactNode } from "react";

export function StepDots({ currentIndex, total }: { currentIndex: number; total: number }) {
  return (
    <div
      role="progressbar"
      aria-label="Setup phase"
      aria-valuenow={currentIndex + 1}
      aria-valuemin={1}
      aria-valuemax={total}
      className="flex items-center gap-1.5"
    >
      {Array.from({ length: total }, (_, index) => (
        <span
          key={index}
          className={cn(
            "h-1.5 rounded-[3px]",
            index === currentIndex ? "w-7" : "w-2",
            index <= currentIndex ? "bg-sage" : "bg-border",
          )}
        />
      ))}
    </div>
  );
}

export function OnboardingPanel({
  title,
  description,
  children,
  footer,
}: {
  title: string;
  description: string;
  children: ReactNode;
  footer: ReactNode;
}) {
  return (
    <section className="flex w-full flex-col gap-[22px]">
      <div className="mx-auto flex max-w-3xl flex-col items-center gap-2.5 text-center">
        <h2 className="text-[26px] font-semibold tracking-[-0.5px] text-balance">{title}</h2>
        <p className="max-w-2xl text-sm leading-6 text-muted-foreground">{description}</p>
      </div>
      <div>{children}</div>
      {footer}
    </section>
  );
}

export function StepFooter({
  onBack,
  onNext,
  nextDisabled,
  nextLabel,
  onSkip,
  skipLabel,
}: {
  onBack: () => void;
  onNext: () => void | Promise<void>;
  nextDisabled?: boolean;
  nextLabel: string;
  onSkip?: () => void;
  skipLabel?: string;
}) {
  return (
    <div className="flex items-center justify-center gap-[10px]">
      <Button
        variant="outline"
        className="text-[13px] text-muted-foreground hover:text-muted-foreground"
        onClick={onBack}
      >
        Back
      </Button>
      <div className="flex items-center gap-2">
        {onSkip ? (
          <Button variant="ghost" onClick={onSkip}>
            {skipLabel ?? "Skip"}
          </Button>
        ) : null}
        <Button
          variant="outline"
          className="text-[13px] text-muted-foreground hover:text-muted-foreground"
          onClick={() => void onNext()}
          disabled={nextDisabled}
        >
          {nextLabel}
        </Button>
      </div>
    </div>
  );
}

export function ModelLegend() {
  return (
    <div className="flex flex-wrap items-center justify-center gap-4 text-xs text-muted-foreground">
      <span className="flex items-center gap-1.5">
        <Zap className="size-3.5 text-sage" />
        Speed
      </span>
      <span className="flex items-center gap-1.5">
        <CheckCircle2 className="size-3.5 text-sage" />
        Accuracy
      </span>
      <span className="flex items-center gap-1.5">
        <HardDrive className="size-3.5 text-sage" />
        Size
      </span>
      <span className="flex items-center gap-1.5">
        <Star className="size-3.5 fill-sage text-sage" />
        Recommended
      </span>
    </div>
  );
}

export function LoadingState({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground">
      <Spinner />
      {label}
    </div>
  );
}

export function EmptyState({ title, description }: { title: string; description: string }) {
  return (
    <div className="flex flex-col items-center gap-2 py-10 text-center">
      <div className="flex size-10 items-center justify-center rounded-xl bg-muted text-muted-foreground">
        <Server className="size-5" />
      </div>
      <p className="font-medium">{title}</p>
      <p className="max-w-sm text-sm text-muted-foreground">{description}</p>
    </div>
  );
}
