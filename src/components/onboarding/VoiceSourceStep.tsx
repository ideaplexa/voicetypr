import { FrameHeading, PrimaryButton, Radio, machine } from "@/components/onboarding/Frame";
import type { SourceType } from "@/components/onboarding/onboardingTypes";
import { cn } from "@/lib/utils";
import { Cloud, Laptop, Network, type LucideIcon } from "lucide-react";

const CHOICES: { source: SourceType; icon: LucideIcon; title: string; line: string }[] = [
  { source: "local", icon: Laptop, title: `On this ${machine}`, line: `Speech is recognised on this ${machine}. Works offline once a model is downloaded.` },
  { source: "cloud", icon: Cloud, title: "Cloud, with your key", line: "Your audio goes to the provider you add a key for." },
  { source: "remote", icon: Network, title: "Another computer on your network", line: "A Mac or PC running Voicetypr with sharing on." },
];

/** 02 Where should your voice become text: the three places dictation can run. */
export function VoiceSourceStep({
  sourceType,
  busy,
  onConfirmSource,
  onNext,
}: {
  sourceType: SourceType;
  busy: boolean;
  onConfirmSource: (source: SourceType) => void;
  onNext: () => void;
}) {
  return (
    <section className="flex w-[600px] flex-col items-center gap-[30px]">
      <FrameHeading
        title="Where should your voice become text?"
        line={`Most people start on this ${machine}. You can change this any time in Settings.`}
      />
      <div role="radiogroup" aria-label="Where your voice becomes text" className="flex flex-col gap-3 self-stretch">
        {CHOICES.map(({ source, icon: Icon, title, line }) => {
          const on = sourceType === source;
          return (
            <button
              key={source}
              type="button"
              role="radio"
              aria-checked={on}
              onClick={() => onConfirmSource(source)}
              className={cn(
                "flex items-center gap-4 rounded-[14px] px-5 py-[18px] text-left",
                on ? "border-[1.5px] border-sage bg-sage-bg" : "border border-border bg-card hover:bg-accent",
              )}
            >
              <span className={cn("flex size-10 shrink-0 items-center justify-center rounded-[11px]", on ? "bg-card text-sage" : "bg-secondary text-foreground")}>
                <Icon className="size-5" />
              </span>
              <span className="flex min-w-0 flex-1 flex-col gap-1">
                <span className="text-[16.5px] leading-[1.2] font-semibold text-foreground">{title}</span>
                <span className="text-sm leading-[1.4] text-muted-foreground">{line}</span>
              </span>
              <Radio on={on} />
            </button>
          );
        })}
      </div>
      <PrimaryButton disabled={busy} onClick={onNext}>Continue</PrimaryButton>
    </section>
  );
}
