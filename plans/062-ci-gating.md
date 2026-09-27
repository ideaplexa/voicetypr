# Plan 062 — CI gating and compute policy

Status: IMPLEMENTED — local workflow validation passed; remote checks pending.
Base: main `4f9e497d`. Separate from PR #140 product/runtime fixes.

## Observed waste

PR source changes start frontend, two macOS builds, Windows, and Store MSIX
concurrently. Failed cheap checks therefore burn native build minutes. CI and
Store have no superseded-run cancellation. Releases are already manual.
GitHub reports main is unprotected and has no rulesets (2026-09-06).

## First shippable change

- Cancel superseded CI/Store runs for the same PR/ref; keep workflows isolated
  so CI never cancels release signing or Store and vice versa.
- Gate macOS/Windows builds on successful workflow and frontend checks.
- Give Store its own cheap frontend prerequisite before Windows provisioning.
- Keep conservative application classification and automatic full native
  validation for application changes.
- Add workflow_dispatch to CI for an explicit full run of a chosen revision.
- Do not change release triggers, signing, updater manifests, artifacts, or
  runner provider. No Depot account/billing changes.

## Follow-up gate before manual-only native validation

Automatic native checks cannot be removed until main requires trustworthy
validation of the current PR head/base. A persisted label or an old green run
is insufficient. Use explicit maintainer dispatch, immutable checkout identity,
fail-closed current-head/base verification and required checks with strict
base currency. Test stale-head, changed-base, fork permissions and failed/
canceled jobs before activating the policy. Preserve existing protection fields
if configuring enforcement. Depot eligibility and measured speed/cost remain
separate from this correctness gate.

## Validation

Run workflow helper contract tests, JavaScript syntax checks and actionlint.
Use the PR's workflow checks and a manual CI run to verify the updated job graph;
confirm native jobs queue only after successful cheap checks. No product
hardware smoke is claimed by CI policy validation.

Local validation: all 18 workflow-helper tests pass; pinned actionlint 1.7.7 passes.

## Review corrections — 2026-09-06

Native and frontend conditions use `!cancelled()` rather than `always()` so
superseded running jobs actually stop. GitHub re-evaluates job conditions during
cancellation; unconditional `always()` can keep them alive. See the
[workflow cancellation reference](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-cancellation).
CI and Store checkouts also disable credential persistence before running
repository code. Existing read-only workflow permissions remain intact.

The first corrected manual run (34004045086) stopped native jobs when frontend
preflight caught main's existing dialog-close test failure. The dialog had
already unmounted, contradicting the test's assumption that it must remain
mounted with data-closed. Applied the same test-only correction as PR #140:
wait for the dialog to disappear and verify persisted selection by reopening.
No product code is pulled into this CI change.

Removed CI-only YAML edits from Store triggers: the Store workflow owns its
preflight and does not consume ci.yml, so unrelated CI edits should stay cheap.
