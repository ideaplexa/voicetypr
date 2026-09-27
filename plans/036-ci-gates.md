# Plan 036 (draft) — CI gates hardening

> From the 2026-07-02 workflow audit. CI-only slice (.github/workflows/ci.yml +
> rust-toolchain.toml). NOTE: cannot fully verify CI behavior locally — acceptance is
> yaml-lint/actionlint + a careful diff review; the real proof is the first CI run
> after push (user pushes; we don't).

## Verified problems (audit, file:line)
- Windows job: `cargo test --no-run` only — backend tests never execute on Windows.
- clippy runs only on macOS (ci.yml:98); ubuntu + Windows never linted.
- Both macOS arches run identical full `cargo test` — one redundant.
- setup-node has package-manager-cache: false (ci.yml:23-24 etc.) — no pnpm cache.
- No Swift .build cache; ffmpeg sidecar re-downloaded every run.
- CI `pnpm test` uses bare vitest alias (ci.yml:52) vs explicit `vitest run`.
- No rust-toolchain.toml — CI uses runner-default Rust; release.yml pins stable.

## Changes
1. Windows: `cargo test` for real (keep the run-tests.ps1 TaskDialog workaround from
   CLAUDE.md if needed — verify what src-tauri/run-tests.ps1 does and use it).
2. clippy -D warnings on ubuntu (fast fail) AND Windows; keep macOS.
3. macOS matrix: full cargo test on aarch64 only; intel runs `cargo test --no-run`
   + build (mirror of today's Windows behavior) — rationale: same code, arch-specific
   failures are rare and the release build still gates compile.
4. Caches: pnpm store cache (setup-node package-manager-cache or actions/cache on
   pnpm store path); actions/cache for sidecar/parakeet-swift/.build keyed on
   Package.swift+Sources hash (pairs with 035 incremental build.sh); cache
   sidecar/ffmpeg/dist keyed on the ensure-ffmpeg script hash.
5. `pnpm test` → `pnpm test:frontend` (explicit vitest run) in ci.yml.
6. Add rust-toolchain.toml (channel = "stable"? or pin minor — check what release.yml
   uses and what Cargo.toml rust-version says; prefer explicit pinned minor, e.g.
   the version currently in CI images, and use dtolnay/rust-toolchain@ with that in
   both ci.yml and release.yml).
7. Do NOT touch release.yml beyond the toolchain pin alignment (release refactor is
   its own future slice).

## Acceptance
- actionlint (or yaml parse) clean on modified workflows.
- Diff review: no job removed, only strengthened/cached.
- Local: cargo clippy -D warnings passes on macOS in the worktree (proves the
  codebase survives the stricter gate before CI enforces it) — run it.
- Frontend gates unaffected. Do not commit.
