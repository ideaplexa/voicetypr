# Plan 064 — Depot runners and Intel legacy support

Status: COMPLETE — Depot runners were evaluated end to end and deliberately
rejected for routine use: warm GitHub CI achieved practical parity and the
free standard runners are sufficient. Every workflow in this repository runs
exclusively on GitHub-hosted standard runners; Depot remains an external,
unused service. The repository transfer and canonical URL cutover landed on
PR #140, and the chronology below records how the Depot lanes were proven —
and why they were still not adopted. No release action is authorized here.
Base: `e9a022fc` (portable pilot head `e9a022fc94a4978fc41218b177af368389bc9cb7`).
Depends on: 042 (cache correctness), 062 (cheap-check gates and cancellation).

## Decision

Keep GitHub Actions as the workflow, checks, artifact, signing, GitHub
Release, and updater control plane, and run every job on GitHub-hosted
standard runners. Depot's managed GitHub Actions runners were evaluated as
an acceleration lane for trusted Apple Silicon macOS and Windows x64 jobs,
and the pilots below prove the lanes worked — but routine Depot is rejected:
warm GitHub CI achieves practical parity with the measured Depot runs (same
head `e9a022fc`: Depot 17m00s macOS / 17m29s Windows-16 versus GitHub-hosted
35m29s / 22m35s), GitHub's free Windows runner already beat Depot's
unsuffixed two-core tier, and free standard runners remove metered billing,
allowlist maintenance, and cross-provider toolchain drift. Depot's own CI
product also remains rejected: its sandboxes are Linux-only and its
compatibility matrix does not support `release` events, environments, or
fork PR execution. No workflow references Depot: there are no runner-routing
variables, no dispatch trust inputs, and no runner-group allowlist
dependency, and reintroducing any of them would require a new explicit
decision. The durable hardening stays: runners, matrices, and timeouts are
fixed in the trusted `native-ci.yml` callee, so a caller workflow can never
inject a runner label, timeout, trust boolean, or checkout ref.

Intel macOS becomes legacy/best-effort: no automatic PR lane and no required
merge check. It remains a separate x86_64 artifact in the explicit release
workflow and may be requested by a maintainer in a manual full CI dispatch.
Do not remove `darwin-x86_64` from `latest.json` or strand installed Intel users.

## Evaluation context (historical record)

The public repository now lives at `ideaplexa/voicetypr`. GitHub confirms the
old repository path redirects, PR #140 and releases moved, and all nine Actions
secrets remain associated. The local `origin` uses the canonical organization.
Never recreate `moinulmoin/voicetypr`, which would permanently remove redirects.

Transfer audit:

- The authenticated user is an active `ideaplexa` admin. The destination had no
  conflicting repository. The repository has no GitHub Pages, repository
  webhooks, deploy keys, rulesets, or environments.
- Vercel, Coolify and Depot GitHub Apps have organization-wide repository
  access. Permission is not proof of an active provider project or deployment.
  GitHub reports zero repository deployments. Vercel/Netlify/Railway/Cloudflare
  check suites on the last pre-transfer head were queued with zero check runs.
- Coolify is the intended deployment integration. Cloudflare remains the DNS
  provider for `voicetypr.com`; the repository transfer does not modify DNS.
- Historical changelog links may rely on redirects. Runtime updater endpoints,
  Help links, badges, clone URLs, and manual release scripts use the canonical
  new owner on PR #140 and must land before the next release.
- During the pilots, Depot's Default runner group had public repository
  runner access enabled and a Selected workflows allowlist containing only
  the pinned `native-ci.yml`; every Depot-eligible job was defined inside
  that trusted callee with fixed labels and timeouts, and fork PRs could
  only select literal GitHub-hosted runners with a read-only token and no
  retained checkout credentials. That isolation design was validated, and
  its durable core — routing fixed inside the trusted callee — survives the
  GitHub-only decision.

Provider controls confirmed with Depot during the evaluation: the Startup
plan was active, with monthly caps of Container 5,000, GitHub Actions 20,000
billable minutes, Depot CI 20,000 minutes, and 25 GB each for the GitHub
Actions cache and Depot Cache. This repository no longer uses any of it.

The rollout closed by decision rather than enablement: instead of extending
the runner allowlist or setting routing variables, routine and release Depot
use were rejected and every workflow was pinned to literal GitHub-hosted
labels. No Depot variable, input, or allowlist step remains to perform.

## Workflow contract

- Keep draft PR iteration cheap: workflow/frontend checks run, but native jobs
  wait until the PR is ready. Local commits are free; batch reviewed commits
  into deliberate pushes rather than treating every local commit as a remote
  validation boundary. After a ready PR changes, cancellation ensures only the
  newest pushed revision continues.
- Classify frontend-only changes separately from native/build inputs. Frontend
  source still receives type/lint/test coverage without provisioning macOS and
  Windows. Unknown build inputs fail closed to native validation.
- Every PR keeps GitHub-hosted workflow/frontend prerequisites. Superseded runs
  cancel. Native jobs start only after those prerequisites pass, only for native
  or unknown build inputs, and not while the PR is draft.
- `ci.yml` calls the same repository's reusable `native-ci.yml` with no
  inputs. The callee pins ARM macOS to literal `macos-14` and Windows x64 to
  literal `windows-2022`; no runner-routing variable, dispatch trust input,
  or allowlist dependency remains. Callers cannot inject a runner label,
  matrix, timeout, trust boolean, or checkout ref, and fork pull_request
  events resolve to the same GitHub-hosted runners as trusted calls, so
  untrusted contributors and maintainers share identical infrastructure.
- Automatic CI includes Apple Silicon and Windows x64 only. A manual CI
  dispatch exposes `include_intel`; true adds the existing `macos-15-intel`
  lane. Intel is a direct manual-only GitHub job on `macos-15-intel`.
- Store MSIX is manual-only and accepts only exact lowercase 40-character
  SHAs verified as ancestors of `origin/main` with full history before
  packaging. Its single Windows job runs on literal `windows-2022`. Store
  MSIX is not an updater/release asset.
- Release remains manual and GitHub-hosted: the ARM macOS build job runs on
  literal `macos-14`, Windows on literal `windows-2022`, and Intel stays on
  `macos-15-intel`. `dry_run` builds signed and notarized artifacts while
  the version commit, tag, publish, and GitHub release jobs stay off.
  Prepare, publish, release assembly, beta-channel publication, artifact
  names and updater manifests stay unchanged.
- Native jobs have explicit timeout ceilings (macOS 90, Windows 120 minutes)
  fixed in the trusted callee, so a stuck job cannot run unbounded. Draft
  conversion cancels the previous PR run.

## Support contract

Apple Silicon macOS is fully supported and remains the primary Mac download.
Intel macOS is a legacy download: Whisper remains available; Apple Silicon-only
Parakeet is unavailable; critical compatibility fixes are best-effort. Continue
producing the Intel artifact in releases until a separately announced EOL. A
future EOL needs a final Intel build and a static legacy updater route; it must
not simply delete `darwin-x86_64` from the shared updater manifest.

## Verification

- Workflow helper tests cover documentation/frontend/native fail-closed
  classification and release version/update-manifest contracts.
- Pinned actionlint 1.7.7 validates every GitHub workflow, including missing
  variable fallbacks and runner expressions.
- `pnpm build` passes and now runs in the cheap Ubuntu prerequisite, so
  frontend-only changes cannot skip production bundle validation.
- Independent workflow review is clear after adding that production build; a
  later review found the admission bypass recorded below, now closed.
- Portable secure pilot `34785226804` at head
  `e9a022fc94a4978fc41218b177af368389bc9cb7` passed every scheduled job:
  workflow/front-end passed, the allowlisted Depot macOS lane passed in
  17m00s, and Windows-16 passed in 17m29s; Intel was skipped. Depot labels and
  runner names were observed on the expected Default group.

Local result: 18 workflow helper tests and pinned actionlint 1.7.7 pass after
the pilot corrections. An explicit compiler-rt link completed
`cargo test --no-run` locally, including both application test executables.
The production frontend build and canonical URL checks passed before the pilot.
PowerShell syntax was not executed locally because `pwsh` is unavailable;
the passing remote Windows pilots exercised those PowerShell steps.

The first manual Depot pilot was started before a separate point-of-spend
confirmation and canceled. Workflow/frontend prerequisites passed; its ARM and
Windows native jobs started and were canceled, so no native result is accepted.
Some metered usage may have occurred.

Authorized pilot `34768887709` initially queued because the runner group did not
allow public repositories. After that prerequisite was enabled, both Depot jobs
started. ARM macOS failed during Rust test linking: Depot selected Xcode 16.4,
whose whisper.cpp Metal objects reference `___isPlatformVersionAtLeast`, while
Rust's final link omitted `libclang_rt.osx.a`. CI and both macOS release lanes
now discover and link Xcode's compiler runtime. Windows used the unsuffixed
two-core/8 GB label, reached 100% CPU and 98% memory, and was canceled by the
user after 57m50s; its compile-only Rust tests passed, but the release build did
not complete. Routing was then switched to the deliberate 16-core, 64 GB
`depot-windows-2022-16` tier for the remaining pilots. Store and release
workflows did not run.

Compatibility pilot `34773469966` passed with compiler-runtime linking and
the 16-core Windows label, proving the native lanes run on Depot. It did not
prove fork isolation: Oracle then found a matrix/YAML admission bypass — the
Depot admission expression (runner labels and matrix) lived in caller
workflow YAML that a fork pull request can rewrite, so a fork could steer
its own jobs onto the Default runner group. Compatibility success and
admission security are therefore tracked as separate proofs.

Corrective preflight run `34777780297` failed pinned actionlint with
ShellCheck SC2012 on the `ls`-based Xcode discovery before any native job;
no Depot native job ran. Xcode discovery is now find-based, and the secure
pilot proves that fix remotely.

Secure pilot `34778214659` at head
`2259e34e149c9ea43ded178596c8416a76a97e5b` ran through the pinned reusable
workflow under the Selected workflows allowlist and passed all scheduled
jobs: workflow/front-end passed, the allowlisted Depot macOS lane passed in
8m19s, Windows-16 passed in 17m11s, and Intel was skipped. Depot labels and
runner names were observed on the expected Default group. No Intel, Store,
release, signing, or deployment workload ran in this pilot.

The first automatic GitHub-hosted fallback on the pinned callee
(`34778145500`) exposed a cross-Xcode regression: Xcode 16.2 reported a
nonexistent compiler-runtime path even though that older toolchain neither
ships nor requires `libclang_rt.osx.a`. Compiler-runtime discovery is now
bundle-local and optional: Xcode 16.4 links the real archive, while older
toolchains continue without it. Current-head fallback run `34780572224`
passed all scheduled jobs at `e9a022fc`; GitHub-hosted macOS passed in 35m29s,
Windows passed in 22m35s, and Intel was skipped.

Final current-head Depot pilot `34785226804` then passed the same pinned
portable workflow: macOS passed in 17m00s, Windows-16 passed in 17m29s, and
Intel was skipped. No Intel, Store, release, signing, or deployment workload
ran.

Remote GitHub fallback proof on PR #140 heads `27afadd1` and `c2af36ab`:
Store and Intel stayed off and all five scheduled jobs passed. The first run
exposed `scripts/README.md` as an incorrectly native path, so Markdown anywhere
now uses the documentation fast path. PR #140 still ran ARM/Windows because its
cumulative diff contains application code. That is deliberate: classifying only
the most recent push would let a new documentation commit cancel an unfinished
native run for the preceding code commit. Savings therefore come from draft PRs,
batched reviewed pushes, cancellation of superseded runs, frontend-only PRs, and
manual Store/Intel lanes—not from weakening current-head validation.

## Non-goals

No branch-protection mutation, no release, beta publication, Intel artifact
removal, or legacy updater removal. Re-enabling Depot in any form — routing
variables, dispatch inputs, or runner-group allowlist changes — is a
rejected direction, not a pending rollout step.
