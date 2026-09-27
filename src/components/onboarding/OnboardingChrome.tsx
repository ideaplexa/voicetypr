import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/utils";
import {
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  HardDrive,
  Server,
  Star,
  Zap,
} from "lucide-react";
import type { ReactNode } from "react";

export function StepDots({ currentIndex, total }: { currentIndex: number; total: number }) {
  return (
    <div className="flex items-center gap-3">
      <div className="flex items-center gap-1.5">
        {Array.from({ length: total }).map((_, index) => (
          <span
            key={index}
            aria-hidden
            className={cn(
              "h-1.5 rounded-full transition-all",
              index < currentIndex
                ? "w-1.5 bg-sage/60"
                : index === currentIndex
                  ? "w-5 bg-sage"
                  : "w-1.5 bg-border",
            )}
          />
        ))}
      </div>
      <p className="text-xs tabular-nums text-muted-foreground">
        Step {currentIndex + 1} of {total}
      </p>
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
    <section className="flex w-full flex-col gap-7 animate-fade-in">
      <div className="mx-auto flex max-w-3xl flex-col items-center gap-2.5 text-center">
        <h2 className="text-3xl font-semibold tracking-tight text-balance sm:text-4xl">{title}</h2>
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
    <div className="flex items-center justify-between gap-4">
      <Button variant="outline" onClick={onBack}>
        <ChevronLeft />
        Back
      </Button>
      <div className="flex items-center gap-2">
        {onSkip ? (
          <Button variant="ghost" onClick={onSkip}>
            {skipLabel ?? "Skip"}
          </Button>
        ) : null}
        <Button onClick={() => void onNext()} disabled={nextDisabled}>
          {nextLabel}
          <ChevronRight />
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
