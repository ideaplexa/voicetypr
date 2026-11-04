import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import { ArrowUpRight } from "lucide-react";
import { Button } from "@/components/ui/button";
import { buttonVariants } from "@/components/ui/button-variants";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { openReleaseLink, releaseHighlights, releasePageUrl } from "@/lib/releaseNotes";

interface ReleaseNotes {
  version: string;
  body: string | null;
}

export function WhatsNewDialog({ version, onClose }: { version: string | null; onClose(): void }) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const [result, setResult] = useState<{ requested: string; notes: ReleaseNotes | null } | null>(
    null,
  );
  useEffect(() => {
    if (!version) return;
    let active = true;
    void invoke<ReleaseNotes>("get_release_notes")
      .then((notes) => {
        if (active) setResult({ requested: version, notes });
      })
      .catch(() => {
        if (active) setResult({ requested: version, notes: null });
      });
    return () => {
      active = false;
    };
  }, [version]);
  const loaded = result?.requested === version;
  const notes = loaded ? result?.notes : null;
  const highlights =
    notes?.version.replace(/^v/, "") === version?.replace(/^v/, "")
      ? releaseHighlights(notes?.body)
      : null;
  const url = releasePageUrl(version ?? "");

  return (
    <Dialog open={Boolean(version)} onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent
        ref={dialogRef}
        initialFocus={dialogRef}
        className="flex max-h-[85dvh] min-w-0 flex-col sm:max-w-lg"
      >
        <DialogHeader>
          <DialogTitle>What's new</DialogTitle>
          <DialogDescription>Voicetypr {version?.replace(/^v/, "")}</DialogDescription>
        </DialogHeader>
        <div
          role="region"
          aria-label="Release highlights"
          tabIndex={0}
          className="max-h-[45vh] min-h-0 min-w-0 overflow-x-hidden overflow-y-auto break-words text-sm leading-relaxed outline-offset-4 focus-visible:outline-2 focus-visible:outline-ring [&_h1]:text-lg [&_h2]:text-base [&_h3]:font-semibold [&_p]:my-3 [&_ul]:list-disc [&_ul]:space-y-2 [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5 [&_a]:text-sage [&_a]:underline [&_pre]:overflow-x-auto"
        >
          {!loaded ? (
            <p role="status">Loading highlights…</p>
          ) : highlights ? (
            <Markdown
              skipHtml
              components={{
                img: () => null,
                a: ({ href, children }) =>
                  href?.startsWith("https://") ? (
                    <a
                      href={href}
                      onClick={(event) => {
                        event.preventDefault();
                        void openReleaseLink(href);
                      }}
                    >
                      {children}
                    </a>
                  ) : <span>{children}</span>,
              }}
            >
              {highlights}
            </Markdown>
          ) : (
            <p>This update includes fixes and improvements.</p>
          )}
        </div>
        <DialogFooter className="shrink-0 sm:items-center sm:justify-between">
          <a
            href={url}
            className={buttonVariants({ variant: "link" })}
            onClick={(event) => {
              event.preventDefault();
              void openReleaseLink(url);
            }}
          >
            See full release notes <ArrowUpRight className="size-4" />
          </a>
          <Button onClick={onClose}>Dismiss</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
