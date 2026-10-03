# Plan 082: beta.5, founder feedback on 2.1.0-beta.4 (2026-10-03)

Worktree `worktrees/voicetypr-beta5`, branch `feat/2.1-beta5`, from main `70c904a1` plus the plan-080 note.
Roles: Codex gpt-6.1-sol medium implements. gpt-6.1-sol high or medium reviews. Opus 5.5 does design in Pencil. Sonnet 5.5 researches.
Releases need Windows CI green before merging (it caught 3 Windows-only breaks in beta.4).

## A. Bugs (no decisions needed)

1. **Stale "mic sent silence" card after a good take.**
   - `DeadMicWarn` (silence detector) fired early in a 6.7-min take on a Wireless Microphone.
   - The island stack holds cards while recording, with their clocks paused, so the card appeared right after the paste.
   - Fix:
     - a `Clear` (speech arrived) withdraws the MicSilent note
     - recording-time warnings that no longer apply are dropped when the take ends
     - a live-recording warning shows live; it is not queued for after the take
2. **Wrong app logo on the island.**
   - `audio.rs` only sets `recording_app_context` when the capture succeeds.
   - On failure (Raycast, Spotlight, menu-bar or accessory apps, windows under 50×50), the previous app's hint is reused, because it is only cleared when a transcription consumes it.
   - Fix: clear it before capturing.
   - Icon cache (`pill/icons.rs`): don't memoise failures forever, and refresh the path for the same bundle id.
3. **Insights "Where you talk" has no real logos.**
   - It draws letter tiles, and its colour hash differs from the island's.
   - Fix: `usage_stats` keeps the latest `process_path` per app and the UI shows the real icon. One shared colour function for the letter fallback.
4. **Island app icon:** remove the background and shadow behind the icon.
5. **What's new** reuses the "Successfully updated to vX" popup.
   - `latest.json` `notes` is "See the release notes for $TAG".
   - Fix:
     - the release workflow puts the real release body into `notes`
     - the app shows the release notes in a proper dialog
     - the post-update popup stays separate
6. **About & updates scrolls sideways:** the pane container has `overflow-y-auto` with no x guard. Add `min-w-0` and `overflow-x-hidden`, and verify in the preview.
7. **Recording screen preview:**
   - It is a hard-coded drawing of the old pill. Render the real island renderer in a static preview.
   - Fix the edge-offset border spacing: the `mt-0` override puts the border flush against the content.
8. **Home "Try a test dictation":** show it only until the first successful dictation.
9. **Dictionary layout:**
   - the fixed 650 px table scrolls sideways
   - the editor opens below the table and shifts the layout
   - `setEditing(null)` is called during render
   - The redesign is in C.

## B. Build and CI

1. **macOS runner and SDK.**
   - Releases build on `macos-14` with Xcode 16 (SDK 15.2), so macOS 26+ draws legacy window chrome (old traffic lights).
   - GitHub is also deprecating macOS 14 runners.
   - Move to macos-15 (or 26) with Xcode 26.
   - Risk: Whisper's Metal encode failed with the Tahoe toolchain locally (memory `tahoe-metal-breaks-local-whisper-harness`). Verify Whisper on a CI-built artifact before releasing.
   - Also re-check `align_main_window_controls` (`lib.rs`) against Tahoe's titlebar.
2. **Installer size: reliability first.**
   - Windows is 277 MB: the offline WebView2 runtime (about 130–180 MB), VC++, the Vulkan runtime, a PDB and the Vulkan sidecar.
   - Research the pros and cons of each before changing anything (bootstrapper vs offline; Windows 10 without WebView2; offline and corporate installs).
   - Also look at macOS for safe savings.
   - Founder: reliability beats size.
   - Research result (2026-10-03, estimated shares): WebView2 offline ~176 MB (~64%), VC++ ~18 MB, Vulkan runtime ~13 MB, app plus sidecar plus PDB ~70 MB.
   - **Plan:**
     - Keep the WebView2 offline installer: it's the only mode that installs offline, on LTSC/Server, behind a proxy and in the Store, and the NSIS step skips it when WebView2 is already present.
     - Replace `vc_redist.x64.exe` with app-local CRT and `vcomp140.dll`, as the MSIX script already does (about −17 MB, and one fewer failure-prone hook).
     - Drop `VulkanRT-Installer.exe` (about −13 MB): GPU drivers ship `vulkan-1.dll`, and the app already falls back to CPU with a clear diagnostic.
     - Keep the sidecar and the PDB (the PDB is needed for in-process symbolication).
     - Test on fresh Windows 10/11 VMs, offline, without VC++, and with a standard user.
     - Optional later: a second "slim" installer using `embedBootstrapper`.
   - macOS is already lean (main binary 60 MB, Parakeet sidecar 18 MB); no change.

## C. Design (Opus 5.5 in Pencil, founder approves before building)

1. **Sidebar footer → slim status bar:**
   - licence state on the left
   - on the right, an update check that spins when clicked, shows a download badge when an update exists, and opens an install dialog
   - It must not look like a selected button.
2. **Settings information architecture.** Today:
   - General has only Appearance and Open at login
   - Privacy holds one switch
   - Storage is thin
   - About & updates is thin

   Regroup the panes, and move whatever belongs on the main surface out of Settings.
3. **Border and density pass:**
   - no borders nested in borders
   - one card language (Insights uses rings, others use borders)
   - Transcription, Insights, Recording and Dictionary first
4. **Transcription page:**
   - Spoken language and real-time streaming become **per-model** settings, under the selected model, shown only when the model supports them. English-only models hide language.
   - "Showing live words in the island" is island appearance and moves to Recording → Island.
5. **Island start details** (founder, 2026-10-03): users choose how the island behaves.
   - Expanded: always show the start details.
   - Compact: super minimal, never expands.
   - Keep the smart "when something changes" behaviour as the default, but drop the "first 5 dictations" rule.
   - Rename the options so the choice is obvious.
6. **Dictionary redesign:** a stable layout, an editor in a side panel or dialog, and no sideways scroll.

## D. Models

1. **Whisper Small English:** hide it on macOS (Parakeet covers the middle tier). Keep it on Windows, where it's the middle option for CPU-only PCs until Parakeet ships on Windows. Anyone who already downloaded it keeps using it.

## E. Research

1. **Local transcript formatting vs VoiceInk** (ideas only; VoiceInk is GPL, so no code or prompts):
   - Its output, using the same models, has better spacing, paragraph breaks and punctuation.
   - Compare our pipeline (normalizer, `transcript-text` crate, insertion spacing) on **real speech**: Parakeet V2/V3 and Whisper.
   - The founder uses Parakeet V2.
   - Research result (2026-10-03, from public docs only):
     - With Polish off, we paste almost raw engine text.
     - VoiceInk documents a post-transcription formatting stage: paragraph formatting for long transcripts, filler-word removal, an add-space option, and an on-device "Refine" cleanup model.
   - Our gaps, by likely impact:
     1. No local deterministic cleanup: sentence-start capitals, standalone "i", fillers, stutters.
     2. No paragraph breaks: we discard Parakeet token timings and Whisper segment times, which could drive pause-based paragraphs on long takes.
     3. No leading space from the cursor context: we only add a trailing space.
     4. The Whisper prompt carries vocabulary only, with no punctuated style hint.
     5. The `transcript-text` normaliser skips multi-line text entirely.
   - **Founder decision (2026-10-03): no A/B against VoiceInk.** Get the most out of every model ourselves, with per-model configuration. Our own before/after run on real clips is only a regression guard.
   - **Per-engine work:**
     - **Parakeet V2/V3** (casing and punctuation are native):
       - return token timings from the sidecar for pause-based paragraphs on long takes
       - keep CTC vocabulary rescoring
       - V3 language per model
     - **Whisper:**
       - a short punctuated style hint ahead of the vocabulary prompt
       - keep segment breaks and their times
       - beam search on GPU profiles
       - no_context review on CPU
     - **Deepgram:**
       - already uses `smart_format` and keyterms
       - add `paragraphs=true` and read the paragraph transcript
       - a filler-words option
     - **Soniox:** context and language hints are in place; use its endpoint/sentence info for paragraphs.
     - **OpenAI and Groq:** a prompt (vocabulary) is in place; add the same style hint as Whisper.
   - **Shared local formatting stage** (runs for every engine, with Polish off):
     - sentence-start capitals and standalone "i"
     - optional filler and stutter removal
     - paragraphs from pause timings
     - leading space from the cursor context (needs real-app tests)
     - a per-line normaliser (so multi-line text is normalised too)
     - guards for URLs and code
   - **Per-model settings** (C4), kept few: language, real-time streaming, paragraphs and filler removal, each shown only where the model supports it.

## F. Performance (founder, 2026-10-04: "smooth and fast; performance is never a second thought")

1. **Frontend:**
   - React Doctor scan, then fix every finding:
     - needless re-renders and unstable props
     - context fan-out
     - effects that should be derived values
     - big lists without virtualisation (History, Dictionary)
     - long tasks on interaction
   - Profile the main window and the island for dropped frames. The island must stay on compositor-only animations (transform and opacity), with demand-driven rAF only.
   - Budget: interactions respond within 100 ms, at 60 fps with no long tasks over 50 ms.
2. **Backend (Tauri):**
   - no blocking work on the main thread or in command handlers; heavy work goes in `spawn_blocking`
   - batch events rather than chatty emits
   - cache settings and store reads on hot paths
   - measure command latency for the commands the UI calls on every screen
3. Verify with before/after traces, not impressions.

## G. Island motion polish (founder, 2026-10-04)

1. **Timed-card countdown:** replace the right-to-left line with the island's own border draining, or refilling its ring colour, as the timer.
2. **Transcribing and Polish states:** a richer, purposeful animation instead of the plain spinner and sparkle. Explore agent-era patterns (shimmer text, flowing gradients), staying compositor-only and calm.
3. **Pasted card:** centre the content. The text currently hugs the app icon and leaves the right side empty. Balance it within the island's three zones.
4. **The Escape hint moves out of the island.**
   - "Press Esc again to cancel" or "discard" shows as a small live card just above the island, as in the beta.3 behaviour the founder expects.
   - The island keeps its waveform and timer; the hint never replaces the island content.
   - This is the same live-card path as the recording-time mic warnings (A1).
   - It keeps the 2 s countdown, and the drain animation lives on the card.
