# Plan 075 — Desktop E2E dictation harness (BlackHole)

Status: SPEC — Claude 2026-09-27. Target: runs before every 2.1 beta from beta.2.
Founder approved BlackHole 2ch as the virtual microphone (2026-09-27).

## Goal

Claude can prove a real dictation end to end without a human: hotkey →
speech → pill/preview → text lands in a real app → numbers. Replaces founder
smoke tests for everything except hardware feel.

## Pieces

1. **Audio in:** `ffmpeg -re -i clip.wav -f audiotoolbox -audio_device_index
   <BlackHole> -` plays a real clip into BlackHole 2ch; Voicetypr records from
   "BlackHole 2ch". Clips: the real-speech set (LibriSpeech, MLS DE/ES, long
   English; CC BY 4.0) fetched by a script into `.tmp/e2e-clips/`, not committed.
2. **Isolated profile:** run the debug app with `HOME=<temp dir>` so settings,
   history and secure storage never touch the founder's real profile; symlink
   the downloaded models into the temp profile. Seed settings: microphone =
   BlackHole 2ch, the engine under test, a known toggle hotkey, sounds off.
3. **Hotkey:** post the hotkey with `cliclick` (key down/up) — toggle mode for
   start/stop; hold mode for hold-to-talk cases.
4. **Target app:** a fresh TextEdit document focused before the hotkey; read the
   result with `osascript` (`text of document 1`).
5. **Measure:** hotkey→first audio (`start_to_first_audio_ms`, plan 074), stop →
   text at cursor (poll TextEdit), WER vs reference, preview events seen, pill
   screenshot (`screencapture -l <window id>`), app log excerpts (no content in
   committed reports).
6. **Cases:** each engine installed (Parakeet v3, Unified, Nemotron, Whisper
   where Metal works, Soniox/Deepgram when keys exist) × regular/live preview;
   first-word test (clip starts at the same instant as the hotkey);
   last-word test (stop right at clip end); back-to-back dictations; cancel
   (Escape); no-speech tap.
7. **Output:** `.tmp/e2e/<timestamp>/report.json` + a short markdown summary;
   non-zero exit on regression vs the last green run.

Script: `scripts/e2e/dictation-e2e.mjs` (Node, like `scripts/perf-harness.mjs`). macOS first; Windows later on GitHub runners
with a virtual audio driver (VB-Cable / Scream) and the same case list.

## One-time founder setup

- `sudo killall coreaudiod` (or restart) so BlackHole 2ch loads.
- Allow Terminal (or the Claude Code host) under Privacy → Accessibility
  (cliclick key events) and Automation → TextEdit (osascript).

## Acceptance

- One command runs the macOS case list against a debug build and writes the
  report; two consecutive runs agree (same WER, stop→text within noise).
- Plans 073 and 074 real checks run through it.
