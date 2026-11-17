import { openExternalLink } from "@/components/sections/accountLicenseUtils";

/** The founder's line: a person reads every email sent from here. */
export function FounderLine() {
  return (
    <p className="text-[12.5px] text-muted-foreground">
      Made by Moinul.{" "}
      <button
        type="button"
        className="underline underline-offset-2 hover:text-foreground"
        onClick={() => void openExternalLink("mailto:support@voicetypr.com")}
      >
        I read every email.
      </button>
    </p>
  );
}
