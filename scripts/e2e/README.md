# Desktop dictation E2E (macOS)

Build the debug app, install BlackHole 2ch, `ffmpeg` and `cliclick`, then grant the calling Terminal or agent host Accessibility, Automation → TextEdit, and Screen Recording in System Settings → Privacy & Security. Restart the host after changing permissions. Run `node scripts/e2e/dictation-e2e.mjs --check-setup` first; it reports missing devices and fix steps.

Create an uncommitted real-speech clip directory with `manifest.json` containing entries like `[{"file":"clip1.wav","lang":"en","ref":"spoken reference"}, ...]`. The harness groups clips by language and reuses the first clip for a back-to-back case when a group has only one clip. Do not use synthetic TTS for accuracy decisions. Download the selected local model in Voicetypr before running. Quit the normal Voicetypr instance; the app has a single-instance plugin.

```bash
node scripts/e2e/dictation-e2e.mjs --clips .tmp/e2e-clips --engines parakeet:parakeet-tdt-0.6b-v3,whisper:base.en
node scripts/e2e/dictation-e2e.mjs --clips .tmp/e2e-clips --engine parakeet --model parakeet-tdt-0.6b-v3 --cases regular,live-preview --baseline .tmp/e2e/previous/report.json
node scripts/e2e/dictation-e2e.mjs --clips .tmp/e2e-clips --engine soniox --model stt-async-v5 --secure-store .tmp/e2e-cloud-secure.dat
node --test scripts/e2e/
```

The script seeds an isolated HOME under `.tmp/e2e/<timestamp>/`, sets `CFFIXED_USER_HOME` for FluidAudio's Foundation path lookup, links installed model files into it, checks the app's resolved model path, and writes `report.json`, `summary.md`, and pill screenshots there. It never copies the normal settings or history. For cloud engines, give `--secure-store` a separate, already encrypted `secure.dat` on the same Mac containing `stt_api_key_<engine>`; the script copies it into the isolated HOME and refuses the normal profile's store path. Keep that file outside version control. A fresh profile needs an active online trial for recording; an expired trial or failed license check blocks the run. A missing `start_to_first_audio_ms` timing line is reported as `null` until plan 074 adds it. `preview_events_seen` is also `null` because the app does not currently emit a content-free preview counter in its logs. Compare two real runs before accepting latency stability; the script cannot verify device behavior in CI without BlackHole and permissions.
