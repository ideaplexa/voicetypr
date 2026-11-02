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

## Discipline

- Claim non-trivial work in `plans/README.md`; plans are `NNN-slug.md` (letter
  suffix for follow-ups). Done plans move to `plans/archive/`. Hardware checks
  go in `plans/SMOKE.md`; `NEEDS-SMOKE` means code-frozen and unverified.
- Any code change after a beta is published needs a new `X.Y.Z-beta.N+1`.
  Stable promotes the tested final beta without unrelated changes.
