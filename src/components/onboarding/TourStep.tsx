import { FrameHeading, PrimaryButton, QuietButton, machine } from "@/components/onboarding/Frame";
import { DiscoveryArt, type DiscoveryArtKind } from "@/components/discovery/DiscoveryArt";
import type { ScreenId } from "@/components/navigation";
import { isMacOS } from "@/lib/platform";
import { cn } from "@/lib/utils";
import { ArrowRight, Check } from "lucide-react";

/** The More tour, 10b to 10h: one thing Voicetypr does per screen, each with a section to open after setup. */
export const TOUR: { title: string; line: string; open: string; section: ScreenId; art: DiscoveryArtKind }[] = [
  {
    title: "Fix a word once",
    line: "Correct a name in History and Voicetypr offers to add it to your Dictionary, so it lands right next time.",
    open: "Dictionary",
    section: "dictionary",
    art: "tour_suggestion",
  },
  {
    title: "Say “new line”",
    line: "Say only the command, like “new line” or “new paragraph”, then stop. Turn it on in Settings → Polish → Voice commands.",
    open: "Polish",
    section: "polish",
    art: "voice_commands",
  },
  {
    title: "Turn a recording into text",
    line: "Drop in a recording or a video and get the text from the engine you chose.",
    open: "History",
    section: "history",
    art: "tour_file",
  },
  {
    title: `Share this ${machine}’s engine`,
    line: `Let your other computers on a trusted network use this ${machine} to transcribe. Protect it with a sharing password.`,
    open: "Network sharing",
    section: "network",
    art: "tour_share",
  },
  {
    title: "A different style for each app",
    line: "Polish can write one way in Messages and another in your AI chat or terminal. Pick a style per app.",
    open: "Polish",
    section: "polish",
    art: "tour_styles",
  },
  {
    title: `Everything you said, on this ${machine}`,
    line: "Search, edit, copy or export it in History. If a take can’t be transcribed, retry it while its audio is kept.",
    open: "History",
    section: "history",
    art: "tour_history",
  },
  {
    title: isMacOS ? "Voicetypr in your menu bar" : "Voicetypr in your system tray",
    line: "Start, stop, retry and switch Polish, engine or microphone from the Voicetypr icon.",
    open: "Settings",
    section: "settings",
    art: "tour_tray",
  },
];

const PICTURE = "w-full bg-[linear-gradient(270deg,var(--sage-bg)_0%,var(--secondary)_100%)]";

/** One tour screen: the feature's picture, what it does, and the section to open after setup. */
export function TourStep({
  index,
  remembered,
  onOpen,
  onNext,
}: {
  index: number;
  /** The section Done will open instead of Home. */
  remembered: ScreenId | null;
  onOpen: (section: ScreenId) => void;
  onNext: () => void;
}) {
  const item = TOUR[index];
  const chosen = remembered === item.section;
  return (
    <section className="flex w-[600px] flex-col items-center gap-[34px]">
      {item.art === "voice_commands" ? (
        <div className="self-stretch [zoom:1.6]">
          <DiscoveryArt kind={item.art} className={cn(PICTURE, "h-[150px] rounded-[11px]")} />
        </div>
      ) : (
        <DiscoveryArt kind={item.art} className={cn(PICTURE, "h-[240px] rounded-[18px]")} />
      )}
      <FrameHeading title={item.title} line={item.line} />
      <div className="flex items-center gap-3.5">
        <QuietButton className="text-sage hover:text-sage" aria-pressed={chosen} onClick={() => onOpen(item.section)}>
          {chosen ? <Check aria-hidden="true" className="size-[15px]" /> : null}
          {chosen ? "Opens after setup" : `Open ${item.open}`}
        </QuietButton>
        <PrimaryButton onClick={onNext}>
          Next
          <ArrowRight aria-hidden="true" className="size-[15px]" />
        </PrimaryButton>
      </div>
    </section>
  );
}
