import appIcon from "../../../src-tauri/icons/128x128@2x.png";
import { ArrowRight } from "lucide-react";
import { PrimaryButton, Tag } from "@/components/onboarding/Frame";
import { EXAMPLE, ProofPair } from "@/components/onboarding/Proof";

export function AppMark({ className = "size-14 rounded-[14px]" }: { className?: string }) {
  return <img src={appIcon} alt="" aria-hidden="true" className={className} />;
}

/** 01 Welcome: the promise and its labelled example. */
export function PromiseStep({ busy, onBegin }: { busy: boolean; onBegin: () => void }) {
  return (
    <section className="flex w-[640px] flex-col items-center gap-10">
      <AppMark className="size-[72px]" />
      <div className="flex flex-col items-center gap-3.5">
        <h1 className="text-center text-[54px] leading-[1.04] font-semibold tracking-[-2.2px] whitespace-pre-line">
          {"Say it messy.\nIt lands clean."}
        </h1>
        <p className="text-center text-base text-muted-foreground">Set it up in a few minutes, then hear it work on your own words.</p>
      </div>
      <Tag tone="plain">Example of Polish</Tag>
      <ProofPair raw={EXAMPLE.raw} dropped={EXAMPLE.dropped} clean={EXAMPLE.clean} rawLabel="You say" cleanLabel="It lands" size="sm" arrow />
      <PrimaryButton disabled={busy} onClick={onBegin}>
        Get started
        <ArrowRight className="size-[15px]" />
      </PrimaryButton>
    </section>
  );
}
