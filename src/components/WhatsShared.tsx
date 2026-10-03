import { useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";

export const TELEMETRY_COPY = "Share anonymous crash reports and usage numbers — never your words, audio or app content.";
export function WhatsShared() {
  const [open, setOpen] = useState(false);
  return <>
    <button type="button" className="text-xs underline text-muted-foreground" onClick={() => setOpen(true)}>What's shared</button>
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogHeader><DialogTitle>What's shared</DialogTitle><DialogDescription>
          Anonymous information goes to PostHog in the EU to help us find and fix problems.
        </DialogDescription></DialogHeader>
        <ul className="list-disc pl-5 text-sm space-y-2">
          <li>App version, operating system and device category.</li>
          <li>Feature choices, dictation outcomes, word count ranges and speed numbers.</li>
          <li>Fixed error codes and crash locations inside Voicetypr.</li>
          <li>Random installation and dictation IDs that connect these events.</li>
        </ul>
        <p className="text-sm text-muted-foreground">Never your words, audio, clipboard, prompts, keys, file paths, app names or window titles. No screen recording or session replay. You can turn sharing off anytime in Privacy settings.</p>
      </DialogContent>
    </Dialog>
  </>;
}
