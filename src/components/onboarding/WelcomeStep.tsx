import { Button } from "@/components/ui/button";
import { ChevronRight } from "lucide-react";

export function WelcomeStep({ onNext }: { onNext: () => void | Promise<void> }) {
  return (
    <section className="mx-auto flex w-full max-w-3xl flex-col items-center gap-8 text-center">
      <div className="flex flex-col gap-6">
        <div className="flex flex-col gap-4">
          <h1 className="max-w-3xl text-5xl font-semibold tracking-[-0.045em] text-balance sm:text-6xl">
            Welcome to Voicetypr
          </h1>
          <p className="mx-auto max-w-2xl text-base leading-7 text-muted-foreground">
            Choose where transcription runs, confirm system access, and keep or change your current
            recording hotkey.
          </p>
          <p className="text-sm text-muted-foreground">
            By continuing, you agree to our Terms and Privacy Policy.
          </p>
        </div>
        <div className="flex justify-center">
          <Button size="lg" onClick={onNext}>
            Start setup
            <ChevronRight />
          </Button>
        </div>
      </div>
    </section>
  );
}
