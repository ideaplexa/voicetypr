# Clipboard receipt spike — 2026-11-13

## What was run

Single-file Swift/AppKit probes on this Mac, using only controlled fixture text:

- An `NSPasteboardItemDataProvider` served `public.utf8-plain-text` to an `NSTextView` paste action. The paste received the fixture, the provider ran once, and the pasteboard change count did not change when the data was read.
- A clipboard-manager-like reader called `string(forType:)` before the native paste. It triggered the provider once. The later native paste received the fixture but triggered **no second callback**. The callback identifies a pasteboard, item and type, not the requesting process or destination.
- Provider lifetime was retained after local references were released.
- A child Swift process published a lazy provider to a private pasteboard and called `_exit(0)` before any read. The parent could not retrieve the fixture. Repeating this with eagerly supplied string data survived the same abrupt exit.
- `clearContents()` returned the same change count subsequently observed after `writeObjects()`. Publishing again, even with identical text, changes ownership; reading does not.

The first probe restored the user's original clipboard representations only while its captured ownership token still matched. Other probes used private named pasteboards. No clipboard contents were printed or recorded.

## Decision and limits

A provider callback is a render request, **not** a destination read receipt or proof of insertion. Prefetch by a manager defeats attribution, and outstanding promises lose their text if the owner crashes. The integrator explicitly chose the eager-data fallback: retain the existing 500 ms restore timer, require both OS ownership and current insertion generation, and snapshot every representation of every item. Carry the original snapshot forward only while we still own the prior take. Incomplete snapshots are never partially restored. Keep-in-clipboard, failed/no-permission paste and copy-only delivery leave real data, not promises.

There is no production lazy provider and no no-read timeout. An application reading later than 500 ms can still miss the transcript; this fallback does **not** solve arbitrarily slow readers. An ownership token does not prove a read. `NSPasteboard` also offers no atomic compare-and-swap: checks are made immediately before mutation, but cannot make the OS check/write pair atomic against unrelated processes.

`voicetypr clipboard verify --fixture slow-reader --json` uses a private native pasteboard, a controlled 250 ms reader, and the production transaction policy. It reports content-free booleans for delivery within the grace window, full-format restoration, same-text copy ownership, stale-generation rejection and rapid-take carry-forward. JSON explicitly identifies `eager_timer_fallback`, unsupported readers beyond the restore delay, and lack of target-insertion verification. It does not touch the user's clipboard or initialize the desktop app.

## Still NEEDS-SMOKE

The native probe invoked the AppKit paste action; it did not inject a physical Cmd+V. Real Cmd+V into native, Electron and browser fields, real clipboard managers, images/RTF/files rendered after restoration, and quit/crash during an actual dictation remain unverified. No claim about those application classes is inferred from the native action or manager simulation. The Rust implementation/CLI and behavior tests need the integrator's build; no Cargo build, test, formatter or heavy check was run here.

Windows is deliberately unchanged. The existing `windows` feature set does not enable `Win32_System_DataExchange`, which supplies `GetClipboardSequenceNumber`; no unbuildable Windows API or delayed-rendering implementation was added.
