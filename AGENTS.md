# Voicetypr — agent guide

Desktop dictation for macOS 14+ and Windows 10+: hotkey → speak → text at the
cursor. Local engines (Whisper, Parakeet sidecar), cloud engines (Soniox,
Deepgram, OpenAI, Groq, Cohere), LAN network sharing, optional LLM "Polish",
and a live-preview pill. Tauri v2 + Rust, React 19 + TypeScript, Base UI.

How the code fits together: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).
CI and releases: [`docs/RELEASING.md`](docs/RELEASING.md). Roadmap and
decisions: `plans/MASTER-PLAN-2026-Q4.md`.

## Rules

- Review and tests: use a fresh adversarial review only for risky or core changes (recording and data loss, locks and threading, native code, release and updater, security and keys, cloud engines); small UI and copy changes get a self-check with screenshots. Write tests that would catch a real regression in core behaviour; no showcase tests.
- Follow the founder's requirements exactly; check `plans/` for the task first.
- Never push, release or publish without the founder's explicit instruction.
  Commit locally, one git worktree per slice.
- Green CI/tests prove contracts, not runtime behaviour; anything needing real
  hardware stays `NEEDS-SMOKE` until run.
- Judge engines and accuracy on **real speech**, never the synthetic TTS clips
  in `perf-corpus/synthetic`.
- No transcript, audio, clipboard, prompt, key, path or window title in logs,
  PostHog. Secrets go in `secure_store`, never the `settings` store.
- Keep it simple: existing Tauri plugin/API over custom platform code, no
  speculative abstractions, refactors separate from behaviour changes. Don't
  grow the oversized files listed in the architecture doc.
- Strict TypeScript (no `any`), `@/` imports; extend `src/components/ui/*` by
  composition, never edit them.

## Commands

```bash
pnpm tauri:dev        # full app (macOS builds the Parakeet sidecar)
pnpm check            # typecheck → oxlint → vitest → cargo test → clippy
pnpm exec vitest run  # frontend tests (pnpm test = watch)
pnpm test:backend     # cargo test
cd src-tauri && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check
bash sidecar/parakeet-swift/build.sh release   # Parakeet sidecar
```

Lint is **oxlint** (not ESLint). Rust 1.91.1 (`rust-toolchain.toml`). On
Windows run Rust tests with `src-tauri/run-tests.ps1`.

## Invariants — do not break

1. The real-time audio callback (`audio/recorder.rs`) never allocates or blocks.
2. Post-roll is for user stops only; cancel/auto-stop/errors/shutdown stop now.
3. `start_recording` returns `true` only if this call started the recording;
   hold-to-talk depends on it.
4. Thread the recording generation through async recording work; keep
   `StopInFlightGuard`.
5. Exit teardown order in `lib.rs`: clear `TranscriberCache` → unload ONNX
   Parakeet (terminal shutdown) → drain shadow VAD → stop remote server →
   media-pause cleanup → analytics shutdown (else Metal SIGABRT).
   VAD admission closes first: drain at most 250 ms, absolute exit watchdog 8 s;
   outlasting native work exits without C++ static destruction.
6. `STOP_JOIN_TIMEOUT` (8 s) must cover post-roll + drain + stream drop + finalize.
7. Parakeet sidecar stdout is the JSON protocol: write only via
   `writeProtocolLine`; never print to stdout.
8. The pill is vanilla DOM; keep the `[hidden]` guard in `pill.css`.
9. Settings change in Rust (`commands/settings.rs`) and `src/types.ts` together;
   renamed keys read the old key and delete it on save.
10. Live preview: committed text only grows; stale sessions/revisions are
    rejected (`transcription/stream.rs`).

## Gotchas

- The pill is an NSPanel on macOS (no focus stealing): retest focus and
  back-to-back dictation after window changes.
- Tauri permissions live in `src-tauri/capabilities/`.
- Failing to detect speech is not proof of silence; new pre-engine gates start
  in shadow mode.
- CLI local and remote transcription use the shared writing pipeline (vocabulary,
  library rules and optional Polish); CLI/file jobs have no recording hot-spare lifecycle.
- Base UI, not Radix: tests mock pointer capture and `getAnimations`.
- `dist/` folders are gitignored; a new Windows worktree needs the Vulkan
  sidecar built or copied.
