import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import Markdown from "react-markdown";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";

const openReleaseLink = (url: string) => { void invoke("plugin:opener|open_url", { url, with: null }).catch(() => {}); };

interface ReleaseNotes { version: string; body: string | null; url: string }
export function WhatsNewDialog({ version, onClose }: { version: string | null; onClose(): void }) {
  const [result, setResult] = useState<{ requested: string; notes: ReleaseNotes | null } | null>(null);
  useEffect(() => {
    if (!version) return;
    let active = true;
    void invoke<ReleaseNotes>("get_release_notes").then(notes => {
      if (active) setResult({ requested: version, notes });
    }).catch(() => { if (active) setResult({ requested: version, notes: null }); });
    return () => { active = false; };
  }, [version]);
  const loaded = result?.requested === version;
  const notes = loaded ? result?.notes : null;
  const url = `https://github.com/ideaplexa/voicetypr/releases/tag/v${encodeURIComponent(version ?? "")}`;
  return <Dialog open={Boolean(version)} onOpenChange={open => { if (!open) onClose(); }}>
    <DialogContent className="min-w-0 sm:max-w-xl">
      <DialogHeader><DialogTitle>What's new</DialogTitle><DialogDescription>Voicetypr {notes?.version ?? version}</DialogDescription></DialogHeader>
      <div className="max-h-[55vh] min-w-0 overflow-x-hidden overflow-y-auto break-words text-sm leading-relaxed [&_h1]:text-lg [&_h2]:text-base [&_h3]:font-semibold [&_p]:my-3 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5 [&_a]:text-sage [&_a]:underline [&_pre]:overflow-x-auto">
        {!loaded ? <p role="status">Loading release notes…</p> : notes?.body ? <Markdown skipHtml components={{ img: () => null, a: ({ href, children }) => href?.startsWith("https://") ? <a href={href} onClick={event => { event.preventDefault(); openReleaseLink(href); }}>{children}</a> : <span>{children}</span> }}>{notes.body}</Markdown> : <p>Release notes aren’t available right now. You can read them on the release page when you’re online.</p>}
      </div>
      <DialogFooter><a href={url} className="inline-flex items-center rounded-lg border border-border px-3 py-2 text-sm" onClick={event => { event.preventDefault(); openReleaseLink(url); }}>Open release page</a><Button onClick={onClose}>Dismiss</Button></DialogFooter>
    </DialogContent>
  </Dialog>;
}
