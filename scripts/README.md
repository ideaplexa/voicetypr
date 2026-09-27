# Voicetypr Release Scripts

## Scripts Overview

### Main Release Scripts
- `release-separate.sh` - macOS release script (creates version, builds both architectures, creates GitHub release)
- `release-windows.ps1` - Windows release script (builds NSIS installer, updates existing release)

### Microsoft Store MSIX
- `build-msix-store.ps1` - Windows Microsoft Store MSIX package builder
- Store submission playbook: [`MICROSOFT_STORE_LAUNCH.md`](../MICROSOFT_STORE_LAUNCH.md)

### Supporting Scripts
- `latest.json` for the updater is generated inline by `release-separate.sh`, `release-windows.ps1`, and the release workflow (no standalone script)
- Other scripts - Various build configurations for different scenarios

## Release Paths

**Automated (recommended):** the GitHub Actions release workflow
`.github/workflows/release.yml` (workflow_dispatch) computes the version and tag with
`.github/scripts/release-tool.mjs`, builds macOS aarch64/x86_64 and Windows x64, bumps version
files, publishes the tag, and opens a draft GitHub release with generated release notes. It does
not regenerate `CHANGELOG.md`; released sections are curated by hand on main.


### CI compute policy

GitHub Actions is the workflow and release control plane, and every workflow
in this repository runs exclusively on GitHub-hosted standard runners.
`ci.yml` calls the same repository's reusable `native-ci.yml` with no
inputs; that callee pins Apple Silicon macOS to `macos-14` and Windows x64
to `windows-2022` with fixed 90/120-minute timeouts, and callers cannot
inject a runner label, matrix, timeout, trust boolean, or checkout ref.
There are no runner-routing variables or dispatch inputs.

- Regular PR CI omits Intel. Use the CI workflow's manual `include_intel`
  option for an on-demand compatibility build; every release still produces
  the legacy x86_64 artifact.
- Draft PRs and frontend-only changes skip native runners. Commit locally as
  needed, then push reviewed checkpoints. Superseded runs cancel.
- Store MSIX packaging is manual-only, requires the exact 40-character commit
  SHA, and is release-candidate validation, not a normal PR check; a preflight
  trust gate verifies that SHA is an ancestor of `origin/main` before the
  Windows packaging job runs, so only reviewed commits on main can be packaged.
  Its single Windows job runs on GitHub-hosted `windows-2022`.

Depot's managed runners were evaluated as an acceleration lane and rejected
for routine use: warm GitHub CI achieves practical parity with the measured
Depot pilots, and free standard runners are sufficient. Depot's own CI
product was separately rejected because it is Linux-only. Depot remains an
external service unused by this repository. See
`plans/064-depot-runners-intel-legacy.md`.

**Manual (local scripts):** the per-platform scripts below.

1. **macOS Release** (creates the initial release):
   ```bash
   ./scripts/release-separate.sh [patch|minor|major]
   ```
   - Bumps version in package.json
   - Updates Cargo.toml and tauri.conf.json
   - Creates git tag
   - Builds both Intel (x64) and Apple Silicon (aarch64) binaries
   - Creates GitHub draft release with macOS artifacts
   - Generates initial latest.json with macOS platforms

2. **Windows x86_64 Release** (adds to existing release):
   ```powershell
   .\scripts\release-windows.ps1 [version]
   ```
   - Reads version from package.json (or uses provided version)
   - Verifies the GitHub release exists
   - Builds the Windows x64 NSIS installer (CPU-safe main app + optional x86_64 Vulkan sidecar)
   - Uses `src-tauri/tauri.windows.conf.json`, which is x86_64-only (Windows ARM64 stays CPU-only)
   - Bundles VC++ and Vulkan Runtime installers as best-effort post-install steps after Authenticode publisher verification
   - Signs the installer and updates latest.json with the Windows platform
   - Uploads the installer, signature, and latest.json to the existing release

### Environment Variables

**macOS (release-separate.sh)**:
- `APPLE_SIGNING_IDENTITY` - Apple Developer signing identity
- `APPLE_API_KEY` + `APPLE_API_ISSUER` - API key authentication (preferred)
- OR `APPLE_ID` + `APPLE_PASSWORD` + `APPLE_TEAM_ID` - Apple ID authentication
- `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH` - Tauri update signing

**Windows x86_64 (release-windows.ps1)**:
- `VULKAN_SDK` - Path to Vulkan SDK (required to build the optional x64 GPU sidecar)
- `VULKAN_RUNTIME_VERSION` or `VULKAN_VERSION` - Vulkan Runtime version for bundling (defaults to SDK folder name)
- `CARGO_TARGET_DIR` - Optional short build output path (honored for target-specific sidecar and main app builds)
- `TAURI_SIGNING_PRIVATE_KEY_PATH` - Preferred Tauri update signing key path
- `TAURI_SIGNING_PRIVATE_KEY` - Tauri update signing key content, written to a temporary key file when no path is set
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` - Password for signing key (if needed; empty passwords are supported)
- If neither signing key env var is set, the script falls back to `%USERPROFILE%\.tauri\voicetypr.key`
- `GITHUB_TOKEN` - GitHub authentication (usually handled by gh CLI)

### Windows x86_64 Build Prerequisites

This release path builds an x64 installer with an optional x86_64 Vulkan sidecar using the `x86_64-pc-windows-msvc` target.
Windows ARM64 builds are CPU-only and must not use `src-tauri/tauri.windows.conf.json`.
The release script and GitHub workflow verify downloaded VC++ and Vulkan Runtime installers with Authenticode before bundling them. GPU acceleration still depends on compatible GPU drivers/runtime availability; CPU fallback remains the safe path.


**1. Vulkan SDK**
Download from https://vulkan.lunarg.com/sdk/home and ensure `VULKAN_SDK` is set.

**2. Windows MAX_PATH Limitation**
Windows has a 260-character path limit. When using git worktrees or long paths, set a short target directory for both the sidecar and main app builds:
```powershell
$env:CARGO_TARGET_DIR = "C:\tmp\vt-target"
.\scripts\release-windows.ps1 -SkipPublish
```
This is especially important for worktrees where paths become very long.

## Important: AppleDouble Files Fix

### The Problem
macOS creates hidden AppleDouble files (prefixed with `._`) when creating tar archives. These files store extended attributes and resource forks. When Tauri's updater tries to unpack these files, it fails with errors like:

```
failed to unpack `._voicetypr.app` into `/var/folders/.../T/tauri_updated_app.../`
```

### The Solution
1. **Archive creation excludes**: `release-separate.sh` and the release workflow create updater archives with `COPYFILE_DISABLE=1 tar -czf ... --exclude='._*' --exclude='.DS_Store'`; prevention is built into the packaging steps (there is no separate fix script)

### Manual Fix (if needed)
If you need to fix an existing archive:
```bash
COPYFILE_DISABLE=1 tar -czf fixed.tar.gz --exclude='._*' --exclude='.DS_Store' Voicetypr.app
```

This ensures the Tauri updater can successfully unpack and install updates on all macOS systems.

## Known CI Failure: Intel Artifact Upload ENOTFOUND

The manual `build-macos-intel` CI job (runner `macos-15-intel`) can fail in its `Upload preview artifact`
step with `Failed to CreateArtifact: Unable to make request: ENOTFOUND`. That is a transient,
runner-side DNS failure reaching GitHub's artifact storage; the build itself completed. Re-run
the failed job. This is external network noise: do not change build or packaging code for it.

Packaging and notarized artifacts have no automated test suite. Green CI establishes compilation
and automated contracts only; packaged-app validation is the manual smoke matrix
(`plans/SMOKE.md`).