# CI and releases

## CI

- `ci.yml`: change classifier → workflow lint → frontend (oxlint, tsc, vitest,
  build) → `native-ci.yml` (macOS: cargo test, workspace clippy, release build;
  Windows: clippy, `run-tests.ps1`, NSIS installer build, no-Vulkan-import check,
  installer install check). Native
  jobs skip draft PRs; `windows-check.yml` is a faster Windows lane you can
  dispatch on any branch.

## Releasing

- `release.yml` (manual dispatch): `channel` stable|beta, `release_type`
  current|patch|minor|major, `beta_number`, `dry_run`.
  - Stable pushes the version bump and tag to the dispatched branch (`main` or
    `hotfix/*`); beta pushes only the tag, leaving the branch unchanged.
  - The release is created as a **draft**; publishing is a separate,
    founder-approved step (`gh release edit <tag> --draft=false`).
  - The job fails if the dispatched branch moves during the build — don't push
    to that branch while a release runs.
  - `CHANGELOG.md` is edited by hand before releasing.
  - **Updater highlights are not the release body.** `latest.json` → `notes`
    must contain a short, plain summary (3–5 Markdown bullets), never the full
    release body, headings, images or a commit list. Full notes live only on
    the GitHub release page. Example:
    ```text
    - More reliable back-to-back dictation.
    - Clearer update controls and release highlights.
    - Fixes for microphone recovery.
    ```
  - The workflow defaults `notes` to `See the release notes for vX`. Before
    publishing the draft, the release operator should replace this field in
    its `latest.json` asset with reviewed highlights and re-upload that asset
    (preserve version, signatures, URLs and publication date). Do this before
    publishing so the rolling beta channel copies the same summary.
    A placeholder, missing notes or an offline request gets a friendly generic
    overview in What's new, always with a link to the installed release.
    The tagged manifest download is capped at **64 KiB**, including responses
    without a trustworthy Content-Length; oversized or malformed responses use
    that same fallback. Keep the complete JSON asset within this limit.
  - What's new reads `notes` from the **installed version's tagged**
    `releases/download/v<version>/latest.json`, never a rolling channel or the
    GitHub release body. The workflow stages the full generated/existing body
    in a separate `release-body` artifact for `body_path`; do not reconstruct
    it from manifest notes. The update confirmation never displays `notes`.
  - Update confirmation actions are **Update**, **View notes**, and **Cancel**.
    Cancel defers installation and keeps the update available. Its literal
    caption is required for macOS Escape: rfd 0.16.0 adds custom NSAlert buttons
    without key equivalents, and AppKit automatically binds Escape to "Cancel".
    On Windows, rfd enables TaskDialog cancellation; Escape returns `IDCANCEL`,
    mapped through tauri-plugin-dialog to the cancel caption, never Update.
  - Both macOS DMGs are separately notarized, stapled and Gatekeeper-checked before upload, including in dry runs.
- `update-beta-channel.yml` updates the rolling `beta` prerelease manifest when
  a release is published (the public URL is CDN-cached for a few minutes).
  Updater endpoints (`commands/updater.rs`): stable
  `releases/latest/download/latest.json`; beta
  `releases/download/beta/latest.json`. Betas must be GitHub prereleases.
- `store-msix.yml` builds the Store MSIX for an exact reviewed commit on `main`
  (artifact only; submission in Partner Center is manual). Store installs are
  Microsoft-signed; the direct Windows installer is not Authenticode-signed
  yet (SmartScreen warning).

## Windows runtime (direct installer)

- Windows x64 Parakeet uses the official Microsoft ONNX Runtime 1.28.0 CPU
  DLL, staged by `scripts/stage-windows-ort.cjs` using the pinned archive/DLL
  SHA256 values in `src-tauri/models/onnxruntime/provenance.json`. NSIS and MSIX
  put `onnxruntime.dll` beside the executable with its notices. Loading is lazy
  and CPU-admitted; no static ORT or DirectML. CI checks native imports and the
  main executable's linker MAP to enforce that design.
- Windows builds use `.github/actions/ggml-cpu-baseline`; Windows artifact
  uploaders must pass `.github/actions/ivy-bridge-smoke` (Intel SDE, no AVX2)
  first. This CLI admission check is not real-speech or installed-app smoke.
- No `vc_redist.x64.exe` and no Vulkan runtime installer ship or run.
  `voicetypr.exe` is built with the static CRT, but whisper-rs OpenMP imports
  `vcomp140.dll`, so the build stages the Visual C++ runtime DLLs (CRT + OpenMP)
  from the Visual Studio redist folder:
  `pwsh scripts/stage-windows-vc-runtime.ps1 -OutDir src-tauri/windows/resources/vcredist`
  (run by `release.yml`, `native-ci.yml` and `scripts/release-windows.ps1`; it
  fails if `vcomp140.dll` or `vcruntime140.dll` is missing). The folder is
  gitignored and bundled via `tauri.windows.conf.json`.
- `src-tauri/windows/installer-hooks.nsh` copies those DLLs beside
  `voicetypr.exe` after install (app directory precedes System32 in the DLL
  search order), aborts with a clear message if `vcomp140.dll` or
  `vcruntime140.dll` is absent, and deletes them on uninstall by iterating the
  bundled folder.
- The Vulkan loader (`vulkan-1.dll`) comes from the GPU driver. Without it the
  app falls back to CPU and says Vulkan is not installed.
- `scripts/check-windows-install.ps1 -Installer <setup.exe>` runs in
  `release.yml` (before signing/staging) and `native-ci.yml`: silent install,
  `dumpbin /dependents` of `voicetypr.exe`, the sidecar and every installed DLL
  (each import must be a Windows system DLL on the script's allowlist or ship in
  the install dir; Visual C++ runtime DLLs are never allowlisted), `voicetypr.exe
  --version` exits 0, silent uninstall leaves none of the runtime DLLs. If it
  flags a genuine Windows system DLL, add it to the allowlist in the script.
  The runner has a system redist, so the dependency check is the real proof; a
  fresh VM without VC++ stays NEEDS-SMOKE (`plans/082`, B.2).
- `windows-check.yml` only stubs the resources (`vcredist/placeholder.dll`,
  `voicetypr.pdb`, a sidecar) so clippy and tests compile; it builds no installer.

macOS native builds apply the CPU-only whisper.cpp VAD patch through
`.cargo/config.toml`. Preserve `--cfg voicetypr_vad_cpu_only_v1` when appending
Rust linker flags and keep the CPU-VAD cache epoch separate from pre-patch
artifacts; the native link sentinel rejects stale archives. No ORT ships on Mac.

## Discipline

- Claim non-trivial work in `plans/README.md`; plans are `NNN-slug.md` (letter
  suffix for follow-ups). Done plans move to `plans/archive/`. Hardware checks
  go in `plans/SMOKE.md`; `NEEDS-SMOKE` means code-frozen and unverified.
- Any code change after a beta is published needs a new `X.Y.Z-beta.N+1`.
  Stable promotes the tested final beta without unrelated changes.
- Every release must be significant (founder rule): batch finished work into
  one beta instead of shipping one small fix at a time, since each release
  goes through the full build, check and publish steps.
