# Plan 035 — Sidecar build DX: incremental Swift builds, single build owner

> From the 2026-07-02 workflow audit. Pure build-system slice: no product code
> changes, sidecar binary output must be byte-equivalent in function. Grounding
> verified against main @ af63ab1.

## Verified problems

1. `sidecar/parakeet-swift/build.sh:19` runs `rm -rf "$SCRIPT_DIR/.build"` on EVERY
   invocation — full clean Swift+FluidAudio rebuild (minutes) whenever build.rs
   triggers, defeating SwiftPM incremental compilation. The comment on line 18
   ("keep dist directory for incremental builds") contradicts the code.
2. Double build during `tauri build`: `src-tauri/tauri.macos.conf.json:5`
   beforeBuildCommand runs `pnpm run sidecar:build` (→ scripts/build-parakeet-sidecar.sh)
   AND `src-tauri/build.rs` also runs `sidecar/parakeet-swift/build.sh`. Two diverged
   scripts (build.sh cleans .build; build-parakeet-sidecar.sh:22 does not; the latter
   also carries a dead legacy Python/MLX/pyinstaller fallback for a sidecar/parakeet
   dir that no longer exists).
3. `build.rs:14` unconditionally emits `cargo:warning=Building Swift Parakeet
   sidecar...` — warning spam on every triggered build.
4. build.rs rerun-if-changed lines (build.rs:78-80) are correct — keep them.

## Changes

1. **build.sh: make incremental the default.** Remove the `rm -rf .build` from the
   default path; add an optional `--clean` flag that restores it (for CI/toolchain
   corruption). Fix the stale comment. Keep dist output path + verification identical.
2. **Single canonical sidecar build script.** Make `scripts/build-parakeet-sidecar.sh`
   a thin wrapper that delegates to `sidecar/parakeet-swift/build.sh` (forwarding a
   --clean flag if present) and DELETE its dead legacy Python/MLX/pyinstaller path
   (verify sidecar/parakeet really doesn't exist first). package.json `sidecar:build`
   keeps working.
3. **Kill the double build.** Verify tauri build ordering (beforeBuildCommand →
   cargo build [build.rs] → bundle): if build.rs reliably produces the dist binary
   before bundling on macOS, remove the sidecar step from
   tauri.macos.conf.json's beforeBuildCommand (keep any frontend build steps).
   If ordering is NOT safe, instead make build.sh skip-if-up-to-date (compare newest
   mtime of Sources/Package.swift/build.sh vs dist binary) so the second invocation
   is a no-op — choose based on what you verify, and say which you chose and why.
4. **Quiet the warning.** Only emit the cargo:warning lines when a build actually
   runs (and consider log-level: warnings are appropriate on failure; a normal build
   message can be a plain println to build script stdout).

## Acceptance

- `cargo build` twice in a row in src-tauri: second run does NOT rebuild Swift
  (prove via timing or build output).
- `touch sidecar/parakeet-swift/Sources/main.swift && cargo build`: Swift rebuilds
  INCREMENTALLY (no .build wipe — prove .build dir survives and rebuild is fast).
- `./sidecar/parakeet-swift/build.sh --clean` still does a clean build.
- `pnpm run sidecar:build` works and produces the dist binary.
- cargo test passes (1185).
- Windows path untouched (build.rs Windows branches, if any, unchanged).
- Do not commit.
