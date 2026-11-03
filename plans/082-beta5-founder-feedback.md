# Plan 082: beta.5, founder feedback on 2.1.0-beta.4 (2026-10-03)

Worktree `worktrees/voicetypr-beta5`, branch `feat/2.1-beta5`, from main `70c904a1` plus the plan-080 note.
Roles: Codex gpt-6.1-sol medium implements. gpt-6.1-sol high or medium reviews. Opus 5.5 does design in Pencil. Sonnet 5.5 researches.
Releases need Windows CI green before merging (it caught 3 Windows-only breaks in beta.4).

## Status (2026-10-15)

Done on the branch (Codex-reviewed unless noted):
- Island: pre-created hidden window plus the debug alpha A/B (`44663698`, `a4939c3f`); polish (`9c04d60a`): three zones, centred result cards, outline countdown, Compact/Smart/Expanded, dB level mapping with display auto-gain; review fixes `81fa9f32`.
- App: Home overview, sidebar status bar, Settings 9 → 7 (`65d077be`, fixes `81fa9f32`).
- CI: arm64 macOS on macos-15 with Xcode 26 (`b811b9e4`).
- Built, awaiting review: the shared formatting stage (E: capitals, fillers, paragraphs, per-line normalising, timings from Whisper, Parakeet TDT and Deepgram, Whisper/OpenAI/Groq style hint); Transcription per-model options, the Recording → Island live-words switch, the lean tray; frontend perf F9/F10.
- Decision: the per-model options are global settings shown under the selected model, only where they apply (no per-model store; a later change if users ask).
- Decision: the outline countdown animates an SVG stroke (paint, not compositor). It is a small card shown for 2–2.4 s; measure it in the real-app perf run and switch to transformed segments only if frames over 16 ms show up.

Left:
- Founder: pick the Transcribing and Polishing waiting looks; OK to quit Voicetypr for the real-app A/B (hide vs alpha) and perf/screenshot pass.
- Design: Dictionary redesign and the border/density pass (Pencil, founder approval).
- Perf: F8 implemented (PNG/base64 workers, bounded path/mtime cache; awaiting review,
  real-app frame timing **NEEDS-SMOKE**); the +2.4 MB/take memory investigation remains.
- Voice-processing input experiment on real speech.
- Windows installer: done in CI (app-local CRT + vcomp140, no `vc_redist`/VulkanRT, install check); fresh-VM test stays NEEDS-SMOKE (see B.2 status).
- Leading space from the cursor context (needs real-app tests).
- Release: Whisper Metal check on a CI-built app, full-branch Codex review, Windows CI, PR, beta.5.

## beta.5 scope addendum (2026-10-31)

Founder decision: finish these in order so beta.5 ships clean. This supersedes the beta.6 placement in 083.
Source line numbers below are review anchors, not claims about the changing worktree.

1. **Finish the 2.0.10 fold-in.** In progress: Claude hot spare, capability-gated
   `--disable-slash-commands`, pi/omp model cache and the stale AGENTS.md local-CLI note.
   **NEEDS-SMOKE:** supported CLI versions, model refresh, hot-spare isolation and quit on both OSes.
2. **Agent-CLI argv exposure fix, moved from beta.6.**
   - Slice 2b: private instruction files now live under per-user LocalAppData
     (`agent-input`), with a background startup sweep of directories older than
     15 minutes there and in legacy system temp. Windows inherits AppData ACLs:
     an owner-only protected DACL needs new Security_Authorization crate features,
     so customized broad AppData ACLs remain a documented residual privacy risk.
   - Dynamic system prompts for Claude/pi/omp and Codex `developer_instructions` go via argv
     (`ai/agent_cli.rs` around 523). omp's transcript and OpenCode's prompt/transcript also go via argv
     (around 1137). The hot spare reuses those args.
   - Move dynamic content to stdin, preserving system/user roles and injection defenses. Keep private
     content out of environment, shell strings, errors, debug argv and temp files too.
   - Stdin support is confirmed for Claude, Codex and pi; for omp, per upstream source.
     OpenCode 2.2.3 `run` stdin is unconfirmed: verify per version or use ACP stdio.
   - Tests capture the child's argv and assert no canary prompt or transcript appears, including hot-spare
     launches. Cover EOF, backpressure, timeout, cancellation and semantic/injection regressions.
   - **NEEDS-SMOKE:** macOS/Windows process inspection, supported CLI versions, Unicode/multiline input
     and child cleanup. This blocks beta.6 agent context and warm-session work.
3. **Recordings privacy.**
   - Capture always writes `recordings/recording_*.wav` (`commands/audio.rs` around 5401), even with
     `save_recordings` off. Crashes, cancel-stop errors (around 8652–8669) and no-speech recovery can leave
     files behind. Retention runs only after a successful save (around 4337–4341); no independent sweep.
   - Add startup and periodic sweeps regardless of the setting. Separate capture/recovery storage from
     opted-in recordings. Sweep unfinished files, but skip files still being written. Reset clears both.
   - Slice 3 implementation: per-take `captures/<uuid>/` ownership, with recovery in
     `recovery/`; saved timestamp/UUID filenames stay in `recordings/`. Recovery keeps
     beta.4's three-clip cap and 30-second no-speech / 10-minute failure lifetimes;
     failed storage handoffs now expire too. Accepted retry leases remain owned through
     the retry. The recovery index is still in memory, not a new crash-import feature.
   - Cleanup runs off-thread at startup, every five minutes and after saves, regardless
     of `save_recordings`: ten-minute-old orphan captures (including legacy names),
     abandoned recovery files, and the configured saved-recording retention. Active
     takes, detached writers, retry leases and History re-transcription source WAVs
     are protected under the deletion lock until their job releases ownership.
     Unreferenced saved filenames are not orphans: clearing History alone is not
     consent to delete saved recordings.
     Reset clears both stores of audio; detached writers are deleted after finalization
     (or by a later startup sweep after their process exits).
   - Cloud and remote file uploads use distinct `upload-normalized-<uuid>.wav`
     names. Output ownership travels into the blocking decoder and back with its
     result, so cancellation cleans up even when the worker outlives its caller.
     Active outputs and decoder scratch files are leased; abandoned upload outputs
     and exact `.voicetypr_decode_tmp_<digits>.wav` scratch names in `recordings/`
     have a minimum 24-hour orphan age (ten minutes inside capture take directories).
     Reset also clears these disposable files. Rename failures retain scratch cleanup.
     Slice 6: CLI `record --until-silence` offers finalized failed transcriptions
     under private per-user `cli-recordings/`, prints the retry path on stderr
     (not in logs), and deletes captures on success or capture failure. Help
     documents 24-hour expiry and reset. CLI startup and desktop maintenance
     sweep exact timestamp/hex CLI names (including legacy `recordings/` paths).
     File/server jobs now own recognizable `voicetypr-audio-<uuid>.wav` temps;
     normalization ownership stays with blocking workers through cancellation.
     Exact OS-temp output and decoder-scratch names expire after 24 hours;
     reset clears them without the age gate. Live in-process leases are skipped.
     Anonymous legacy OS-temp `.tmp*` files cannot safely be attributed to
     Voicetypr and are deliberately not swept.
     Slice 6b: accepted CLI-file retries refresh mtime best-effort before app
     startup/engine loading (also on desktop borrowing), restarting the 24-hour
     expiry clock across processes. In-progress CLI captures live in the same
     private directory and have no ten-minute sweep gate; expiry uses last write.
     Explicit reset may remove another process's open capture; the CLI then
     fails without offering a missing retry. Reset now reports content-free
     errors for failed CLI/temp/capture cleanup (except already-missing files)
     while continuing other cleanup; periodic maintenance remains best-effort.
   - **NEEDS-SMOKE:** crash/restart, cancel-stop errors, no-speech recovery, saving off/on, retention,
     active capture during a sweep and reset on both OSes.
4. **Four word-changing formatting bugs from the Codex review of the committed branch.**
   - `writing/library_rules.rs:373`: style-hint stripping deletes real speech matching the hint.
   - `writing/format.rs:556`: filler removal deletes the acronym "ER".
   - `writing/format.rs:623`: stutter removal turns "what it is is" into "what it is".
   - `writing/format.rs:478`: pronoun capitalisation turns "-i" and "/i" into "-I" and "/I".
   - Add regression cases for all four. **NEEDS-SMOKE:** faithful final text on real speech with Polish off,
     plus code/URL insertion on both OSes; synthetic clips do not establish accuracy.
5. **Windows model downloader hardening.**
   - Large v3 Turbo's root cause is unproven; still need the user's exact Windows error.
   - `whisper/manager.rs` does not reject HTTP error status (around 300–305), writes to the final path
     (around 325), leaves partial files on errors (around 346, 362) and shows 100% too early (around 388–389).
   - Use atomic verified staging. Reject bad HTTP status, clean failed staging and publish the final file
     and 100% only after verification succeeds.
   - **NEEDS-SMOKE:** the reported Windows failure once supplied, interrupted downloads, retry, antivirus
     locks, disk-full/error paths and successful model loading. Hardening is not proof of the root cause.
6. **Three minor bugs.**
   - `src/components/sections/ModelsSection.tsx:223`: no language control while remote transcription is active.
   - `cloud_stt/deepgram.rs:84`: the paragraph option does nothing while Deepgram streams.
   - `src/components/sections/useRecentRecordings.ts:127`: an invalid timestamp crashes History grouping.
   - Add targeted regressions. **NEEDS-SMOKE:** remote language selection, streamed Deepgram paragraphs and
     History with invalid timestamps in the packaged app.

beta.6 retains Windows Parakeet v3, default-on context capture when Polish is on, selected-CLI warming,
shadow-first Silero VAD, clipboard receipt restore and Secure Input fallback (083).
Our Liquid local Polish model moves to 084 and ships in a separate beta only after eval passes.

## ★ Island track: priority #1 (founder, 2026-10-04)

The island is the most frequent touchpoint. Every everyday interaction must feel instant and smooth.

| Moment | Target |
|---|---|
| Hotkey press | Visible response in under 50 ms, before the mic is ready. Show a "starting" state on keypress; it no longer waits for first audio. |
| Talking | Bars at a steady 60 fps (no frame over 33 ms); smooth timer. |
| Stop | "Transcribing" shows immediately, with a richer animation (G2). |
| Polish | A distinct, calm polishing animation (G2). |
| Pasted | Centred, balanced content (G3), auto-leaves. |
| Esc, problems, warnings | Live cards above the island; never inside it (G4 done). |
| Hover, click and peek | Smooth open; pick lists respond on the first click. |
| Timed cards | The outline drains as the countdown (G1). |

Order:
1. F0 measurement.
2. Instant appearance.
3. Smoothness (F6: transform/clip only, cached rects, canvas bars fed from a ring buffer).
4. G1–G3.
5. Prove it with before/after numbers plus a cua-driver screenshot pass of every state (debug build with `VOICETYPR_E2E_CAPTURE_PILL`).

### Island track: audio level looks quiet at normal voice (founder, 2026-10-07)

- **Cause:** the wave uses `sqrt(peak)` (`pill/level.rs:54`, `audio/level_meter.rs`), which is close to a linear amplitude. Normal speech peaks at 5–15 % of full scale, so the bars reach only about 25–40 %.
- **Fix (display only):**
  - Map the level on a speech-tuned dB scale (roughly −50 dB → 0, −12 dB → 1, with a soft knee).
  - Add a gentle display auto-gain: the reference follows the take's recent peak over a few seconds, with a floor so silence stays flat.
  - Applies to the island and the Recording meter.
- **Capture is unaffected:** recordings are normalised before transcription.
- **Experiment, measured on real speech before deciding:** macOS voice-processing input (AGC + noise suppression) for quiet or far-field speech.
  - It may change audio quality and duck other audio. Compare WER and loudness on the same real clips.
  - Windows has an equivalent communications/AGC mode.

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
   - **Status (2026-10-15): vc_redist/VulkanRT removal done in CI, NEEDS-SMOKE on real machines.**
     - App-local CRT + OpenMP DLLs via `scripts/stage-windows-vc-runtime.ps1`; NSIS hook copies them beside the exe and removes them on uninstall; both installers dropped. `scripts/check-windows-install.ps1` (release.yml, native-ci.yml) does silent install, on-disk dependency check, `--version`, uninstall. Not yet seen green on CI.
     - NEEDS-SMOKE: fresh Windows 10 and 11 VMs without VC++; offline install; standard (non-admin) user; PC without Vulkan shows the CPU message; PC with a Vulkan GPU still uses the GPU sidecar; upgrade from beta.4 (old uninstaller, then new files); uninstall residue (no DLLs left in the install dir).
   - macOS is already lean (main binary 60 MB, Parakeet sidecar 18 MB); no change.

## C0. Founder design decisions (2026-10-05)

Board: `design/voicetypr-2.1.pen` "Beta.5 · Where should the app open?". Export: `design/exports/beta5/DmqDy.png`.

- **Home stays, as a true overview (option B):**
  - The big "Press ⌥ Space" hero with the setup chips. Remove "Try a test dictation" here; it's in onboarding.
  - **Needs you:** kept recordings with Retry, update ready with Install, setup problems. Shown only when there's something.
  - **Get more out of Voicetypr:** a couple of contextual tips, e.g. turn on Polish, teach it your words.
  - Recent and This week are removed (History and Insights already hold them).
- **Sidebar footer → status bar** (approved):
  - "Pro" as plain text with the licence icon, not a button.
  - Right side: the version, plus an update-check icon that spins while checking, becomes a download badge when an update exists, and opens the install dialog.

- **Placement rule (founder, 2026-10-05): how often a thing changes decides where it lives.**
  - Daily things live in the main pages:
    - shortcut, Press/Hold mode and mic on Recording (plus the Home hero, island peek and tray)
    - engine, language and streaming on Transcription
    - style on Polish
    - words on Dictionary
  - Set-once things live in Settings.
- **Settings, regrouped (approved; board `design/exports/beta5/QJsu7.png`): 9 → 7 panes.**
  - **General:** Appearance, Open at login, Updates (version + Check now, auto-install, channel), Privacy (the one sharing switch + "What's shared").
  - **App shortcuts:** renamed from Shortcuts; extra app actions only, since the dictation shortcut lives on Recording.
  - **Storage.**
  - **Advanced:** Network sharing, CLI & API, Troubleshooting.
  - **Account:** License.
  - The About & updates and Privacy panes are removed. Help and What's new live on the Help page; version and update check live in the sidebar footer.
  - Rows are separated by hairlines with small section labels and no outer card border.

- **Transcription, per-model options (approved; board `design/exports/beta5/LiuA2.png`).**
  - The selected model expands in place:
    - Language: a picker, or an "English only" tag.
    - Real-time words (streaming): shown only on models that stream.
    - Cleanup and formatting (paragraphs at long pauses, remove filler words): shown only where they apply, with per-model defaults tuned to what each model already does.
  - The bottom Spoken language / Live preview cards are removed.
  - **Recording → Island → "Show live words while you talk"** depends on streaming:
    - When the model streams, it's active and the island grows to show the words.
    - Otherwise it's greyed out with a "turn on Real-time words" hint.
    - Off means a compact island, with faster final text still.

- **Island polish (approved; board `design/exports/beta5/woACk.png`):**
  - Listening in three zones: icon left, waveform centred and filling the middle, timer right.
  - Pasted centred: check, "Pasted" and the word count grouped in the middle, icon anchoring the left.
  - Timed cards: the card outline drains as the countdown (replaces the line).
  - Start details: Compact / Smart (default; the "first 5" rule is dropped) / Expanded.
  - The transcribing and Polish animations will be prototyped live (motion) before building.

- **Menu bar / tray, lean (founder approved 2026-10-07):**
  - Keep:
    - status line (Ready / Recording · 0:07 / problem + Fix…)
    - Start Dictation
    - kept recording Retry/Discard (only when present)
    - Polish ▸, Engine ▸, Microphone ▸, Language ▸
    - Copy last dictation
    - Open Voicetypr, Settings…, Check for updates…
    - Quit
  - **Remove:** Live preview, Mode, Transcribe a file, Recent ▸, Paste last, Insights, Help.

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

### F0. Regression vs pre-redesign (founder, 2026-10-04)

The island appears later, and the audio bars sometimes stutter. 2.0.7 felt instant.
- **Measure first** (debug-only instrumentation, content-free):
  - hotkey → island visible (ms)
  - pill frame times while recording (p50, p95 and max; long frames over 16 ms and over 50 ms)
  - pill and main memory across ~50 back-to-back dictations (leak check)
  - idle CPU with the resting dot showing
- Compare 2.0.7, beta.4 and the fixed build on the same machine, using the E2E harness and real speech.
- **Suspects:**
  - the pill animates layout properties and measures every frame (F6)
  - 60 Hz level IPC to both windows (F1)
  - the 30 Hz cursor poll and `set_ignore_cursor_events` on the main thread, which contends with show and position
  - the island shows only after first audio and then plays an opening morph
  - possible DOM or listener growth in the waveform or stack
- **Done means** the numbers match or beat 2.0.7.
- Snapshot: the founder's installed beta.4 idles at about 1% CPU and 251 MB footprint after 16 h.

### F0 baseline (2026-10-04)

Run: debug build, Parakeet v3, 15 back-to-back takes, from `.tmp/e2e/2026-10-04T20-48-39.639Z`.

- **Hotkey → island visible (take 1, pill hidden): 381 ms.**
  - hotkey → Starting: 13 ms
  - Starting → Recording (mic open): **278 ms**
  - Recording → pill shown: 38 ms
  - shown → first visible frame: 52 ms

  The island waits for the mic. Fix: show it on keypress (Starting).
  The takes 2+ metric is invalid: the window was still visible from the previous take. Measure only from a hidden pill.
- **Frames while recording:** p50 17 ms, p95 18 ms, max 26–58 ms, at most one frame over 33 ms per take. Smooth when the machine is idle; the founder's stutter likely happens under load. Making it compositor-only (F6) hardens it.
- **Audio level gaps:** p95 20 ms. DOM is stable at 141 nodes, so there's no pill DOM leak.
- **Main process RSS: 157.7 → 194.8 MB over 15 takes (+~2.7 MB per take).** Run a longer soak and profile before calling it a leak.

### F0 after instant island and compositor motion (commit 404c8c7d, run 2026-10-04T22-12-17.840Z)

- **Hotkey → island, cold first take: 381 → 187 ms.**
  - Show at Starting: 13 ms.
  - Pill window creation and first show: 129 ms. The window is lazily created on the first recording.
  - First frame: 45 ms.
- **Warm (hidden window): about 60 ms**, of which about 49 ms is the native window show.
- **Frames:** max 18–25 ms (was 26–58); over33 = 0 (was 2). p95 18 ms.
- **RSS growth unchanged** (+36.6 MB over 15 takes), so it's not the island. Investigate the main process.
- **Next:**
  1. Pre-create the pill window shortly after startup (idle).
  2. Keep the panel ordered in, invisible and click-through, when idle (as the "always" mode does), so the show is just a repaint (target ~15 ms).
  3. Run a 40-take soak plus `leaks` and `vmmap` on the main process to find the RSS growth.

### Validation of the next island steps (research, 2026-10-05)

1. **Pre-create the hidden pill window** shortly after startup, on the main thread, with `visible(false)`. This is the common overlay pattern. The cost is one idle webview (keep it static while hidden).
   - On WebView2, `hide()` may not idle the engine: consider `put_IsVisible(false)` and `TrySuspend`.
2. **Ordered-in at alpha 0 instead of `orderOut`.** HUD apps use this to avoid WKWebView occlusion throttling, which likely explains most of the 49 ms warm show.
   - Not an Apple-documented recipe, so **A/B-measure hidden vs alpha-0 first-paint latency on real hardware before adopting it**.
   - If adopted: alpha 0 plus `ignoresMouseEvents`, and the collection behaviour CanJoinAllSpaces | FullScreenAuxiliary | Stationary | IgnoresCycle.
   - Keep content protection.
   - Windows: a layered alpha-0 window with `WS_EX_TRANSPARENT`.
   - Spaces, Stage Manager and full-screen behaviour need a smoke test.
3. **Memory** (+2.4 MB per take):
   - Check `vmmap --summary` before and after 20 dictations to separate allocator plateau from unbounded growth.
   - `heap -sortBySize` and `leaks --groupByType`, with MallocStackLogging and `malloc_history`.
   - Instruments Allocations with Mark Generation per dictation.
   - dhat-rs if the growth is in the Rust heap.
   - Eliminate candidates one at a time: TranscriberCache, the store, the pill webview.

### F. Audit result (2026-10-04, static; React Doctor local-only: 64 warnings, 11 perf)

Ranked:
1. **`audio-level`** is emitted at 60 Hz to `main` even when it's hidden or on another screen, and it drives `setLevel`.
   - Emit to `main` only while the Recording screen's meter is mounted.
   - On the frontend, keep the level in a ref and write it in rAF.
2. **SettingsContext reload** sets `isLoading` and `error` on every settings, model, language or device event, so all 47 consumers re-render twice.
   - Set `isLoading` only on the first load, and skip updates when the value is equal.
   - `get_settings` takes the consent lock: check the cost.
3. **ModelManagement, Readiness and ModelAvailability provider values** are not memoised, and `sortModels` runs on every render.
   - Memoise the values.
   - Keep per-model download progress in a small external store.
4. **No code splitting:** every screen and SettingsTab load eagerly.
   - Use `React.lazy` for the secondary screens.
   - Mount the SettingsModal only when it's open.
5. **`get_model_status`** runs twice per event, holds a write lock and does filesystem checks on the async runtime.
   - Share one result, cache it, and move the filesystem work to `spawn_blocking`.
   - Stabilise the 7 listeners.
6. **The pill animates layout properties** (width, height, left, top, radius) and calls `getBoundingClientRect` every frame.
   - Use transform plus clip.
   - Cache rects and compare them numerically.
   - Hover from cached rects.
   - This goes with section G (after the A fix round).
   - The broader compositor and clip rewrite remains beta.6 work. See
     [plan 083 section 13](083-2.1-beta6.md) for acceptance and measurement.
7. **History reads all keys and sorts them on every call.** Keep a key index and count in managed state, and load in `spawn_blocking`.
8. **Icon PNG encoding** runs on the main thread. Add a disk cache, or encode off the main thread.
   - Slice 6: macOS now does only AppKit lookup/rasterization and a bounded
     16 KiB RGBA copy on the main thread. `spawn_blocking` handles filesystem
     validation, PNG/base64 encoding and a 128-entry in-memory path/mtime cache,
     shared by History and the island/Insights icon route. The existing bounded
     island identity cache now invalidates on app/exe mtime changes too.
     Windows extraction/encoding already used `spawn_blocking`.
   - Content-free debug timings separate raster and encode microseconds on misses.
     No before/after UI measurement was run: **NEEDS-SMOKE** for real-app icon
     correctness (alpha/orientation) and frame timing on both OSes.
9. **History rows:**
   - memoised rows with stable callbacks
   - format time once
   - `content-visibility` or virtualisation
10. **Small items:**
    - Dictionary `useMemo`
    - hoist the `Intl` formatter
    - module-level empty default props
    - the TabContainer keydown effect deps
    - `exhaustive-deps` and loading-flag findings

### beta.5 scope update (2026-10-31)

- The founder dropped the Windows Large v3 Turbo download item from beta.5:
  the reporting user never replied. Item 5 above is deferred, not fixed.
  The general downloader weaknesses remain a later item: `whisper/manager.rs`
  does not reject HTTP error statuses, writes straight to the final path,
  leaves partial files after errors, and shows 100% before verification.
- Item 2 implementation: `7350d101` moves Claude/Codex/pi/omp user text to
  stdin and dynamic instructions to 0600 files in a private 0700 directory
  on Unix, owned through cleanup (including Claude's prepared spare).
  Claude uses `--system-prompt-file`; pi/omp resolve `--system-prompt` files;
  Codex uses `model_instructions_file`, replacing coding defaults with the
  dictation prompt at system level rather than demoting it to user content.
  OpenCode 2.x uses a one-shot ACP stdio client with permission denial.
  The single OpenCode 2.2.3 no-message stdin canary failed because no model
  was available, so stdin support remains unproven. OpenCode 1.x retains
  its documented positional-argv exposure; upgrade or choose another CLI
  for private input. No new private content is placed in environment values.
  **NEEDS-SMOKE:** supported CLI versions, packaged macOS/Windows process
  inspection, Windows file ACLs, multiline/Unicode input, prepared-spare
  isolation, cancellation and quit cleanup; a working OpenCode ACP model.
