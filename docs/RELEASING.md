# CI and releases

## CI

- `ci.yml`: change classifier → workflow lint → frontend (oxlint, tsc, vitest,
  build) → `native-ci.yml` (macOS: cargo test, workspace clippy, release build;
  Windows: clippy, `run-tests.ps1`, CPU build, no-Vulkan-import check). Native
  jobs skip draft PRs; `windows-check.yml` is a faster Windows lane you can
  dispatch on any branch.

## Releasing

- `release.yml` (manual dispatch): `channel` stable|beta, `release_type`
  current|patch|minor|major, `beta_number`, `dry_run`.
  - Stable commits the version bump and tag to `main`; beta pushes only the tag.
  - The release is created as a **draft**; publishing is a separate,
    founder-approved step (`gh release edit <tag> --draft=false`).
  - The job fails if `main` moves during the build — don't push to `main`
    while a release runs.
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

## Discipline

- Claim non-trivial work in `plans/README.md`; plans are `NNN-slug.md` (letter
  suffix for follow-ups). Done plans move to `plans/archive/`. Hardware checks
  go in `plans/SMOKE.md`; `NEEDS-SMOKE` means code-frozen and unverified.
- Any code change after a beta is published needs a new `X.Y.Z-beta.N+1`.
  Stable promotes the tested final beta without unrelated changes.
