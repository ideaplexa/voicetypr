# 034 — Compact report problem page

Status: DONE — reviewer pass; local macOS UI smoke and frontend checks passed 2026-07-23

## Problem

The sidebar opens a modal that squeezes contact information, issue details, diagnostics, and failure recovery into one small surface. Reporters need enough room to explain a problem, support needs a reply address, and users should be able to see the computer configuration attached to the report.

## Decision

Replace the sidebar modal with a normal `Report a problem` page. Keep optional name, required email, and required `Describe the issue` fields. Preview the collected OS, architecture, CPU/core count, memory, and GPU details before submission. Gather diagnostics automatically. Direct submission remains primary; the prepared-report copy action appears only if submission fails.

The crash-specific `CrashReportDialog` remains unchanged.

## Acceptance

- Sidebar `Report a problem` opens a dedicated page and shows active navigation state.
- The page contains optional `Name`, required `Email`, and required `Describe the issue` fields.
- The page previews the OS/architecture, CPU/core count, memory, and GPU configuration that will be attached.
- Submission includes contact information, automatically gathered diagnostics, and the recent redacted log.
- Successful submission clears the form and confirms success.
- Failed submission preserves the prepared report and offers `Copy report`.
- Existing report validation, stale-request protection, and copy fallback remain covered.
- The obsolete manual-report modal and its sidebar state are removed.
