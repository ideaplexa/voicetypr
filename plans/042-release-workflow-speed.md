# 042 — Release workflow speed

**Status: DONE — reviewer PASS; workflow gates and cold/warm signed release dry-runs passed**

## Problem

Workflow-only pull requests currently run the complete macOS and Windows native build matrix before the release workflow rebuilds and signs the same platforms. Release preparation logic is duplicated inline, and expensive sidecar inputs are not cached independently from the main Tauri build.

## Locked behavior

1. Every production release still builds, signs, and verifies macOS ARM64, macOS Intel, and Windows artifacts.
2. Pull requests that can affect application behavior still run the existing frontend and native CI matrix.
3. Workflow-only pull requests run dedicated workflow checks and release-logic tests instead of native application builds.
4. Caches may reuse dependency/build outputs but never final signed installers, signing material, or mutable downloaded binaries without integrity checks.
5. Exact tag matching and atomic stable publication remain enforced.

## Acceptance

- A deterministic change classifier distinguishes workflow-only changes from product/build-input changes.
- Required CI checks still report success for workflow-only pull requests without starting native build jobs.
- Release version calculation, beta validation, exact tag matching, and version-file mutation are implemented once and covered by Node tests.
- Windows Vulkan and Apple Silicon Parakeet build caches are keyed by all relevant source/dependency/toolchain inputs.
- FFmpeg download caching preserves the existing checksum/size validation path.
- Workflow syntax and focused tests pass, followed by an independent reviewer pass.

## Implementation

- CI classifies the complete pull-request/push diff. Workflow/docs-only changes run workflow contract checks; any product, build, dependency, script, or sidecar change retains the full frontend/macOS/Windows matrix.
- Release version calculation, exact tag lookup, and package/Cargo version mutation now share `.github/scripts/release-tool.mjs`.
- Windows Rust caching maps the real short-path app and Vulkan sidecar targets; release/CI share their cache while Store builds remain isolated.
- macOS caches SwiftPM outputs by runner architecture, Swift toolchain, package resolution, build script, and sources.
- The Parakeet build script preserves SwiftPM incremental state instead of deleting `.build` before every Cargo invocation.
- Only checksum-pinned Apple Silicon FFmpeg binaries are cached. Restored binaries are revalidated before use; mutable Intel downloads and Windows binaries are not cached by this change.
- The checksum-pinned patched GlitchTip CLI is cached by runner OS/architecture and exact installer source; symbol validation and server-side processing still remain release gates.

## Validation

- Workflow helper tests: 18 passed.
- `actionlint v1.7.7`: passed for every workflow.
- Every `.github/scripts/*.mjs` file passed `node --check`.
- Exact-tag CLI smoke accepted an absent tag and rejected published `v2.0.5`.
- Change-classifier smoke selected the full matrix for this sidecar-changing branch and the fast path for a workflow/plan-only fixture.
- Parakeet sidecar cold build: 56.8 seconds; immediate incremental rebuild: 0.7 seconds; both produced and executed the sidecar successfully.
- FFmpeg first download verified both pinned Apple Silicon SHA-256 values; cached revalidation completed in 0.1 seconds; a deliberately corrupted cached binary was rejected and the restored binary revalidated successfully.
- Frontend verification: lint, typecheck, production build, and 586 tests passed (1 skipped).
- Native integration: `cargo check --lib` passed and exercised the incremental Swift build through `build.rs`.
- Expanded independent current-source review: PASS, with no remaining P0-P2 findings across classifier fail-closed behavior, Windows target mappings, cache trust/keying, artifact paths, PowerShell/YAML, and release gates.
- Corrected cold-cache release dry-run `30766863440` passed all three signed artifact builds in 36m43s.
- Final cache-hit dry-run `30770027517` passed in 14m17s: 61% faster, saving 22m26s. Windows Vulkan compilation fell from 4m24s to 33s, Windows Tauri from 25m02s to 10m22s, and cached symbol upload from about four minutes to 22 seconds on the critical Windows job.
