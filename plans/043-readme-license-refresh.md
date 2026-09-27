# 043 — README and license refresh

**Status:** DONE — GitHub detects AGPL-3.0; README link and formatting validation passed 2026-08-03  
**Scope:** Repository presentation and license metadata only; no application or release behavior changes.

## Problem

GitHub classifies the repository license as `Other` because `LICENSE.md` contains a placeholder instead of the canonical AGPL-3.0 text. The README also describes an older product surface and contains stale privacy, provider, platform, and installation claims.

## Changes

1. Replace the placeholder license file with canonical GNU AGPL v3 text in the conventional `LICENSE` path.
2. Rewrite the README around the current macOS and Windows product: local/cloud transcription, AI formatting, CLI, file transcription, history, network sharing, release channels, Store distribution, and CPU-safe Windows GPU isolation.
3. Keep customer-facing download, website, changelog, documentation, and license links accurate.

## Acceptance

- GitHub can classify the repository as AGPL-3.0 from the canonical license file.
- README claims match shipped Voicetypr 2.0.5 behavior and current privacy defaults.
- Markdown formatting and repository-local links validate.
- Documentation-only PR; no application release or runtime smoke required.
