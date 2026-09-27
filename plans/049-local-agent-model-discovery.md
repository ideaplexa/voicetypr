# Plan 049 — Local agent model discovery

**Status:** DONE — frontend/backend gates, Finder-style installed-CLI smoke, and native CUA model selector/default smoke passed 2026-08-10  
**Priority:** P1  
**Effort:** M  
**Depends on:** 044, 048

## Problem

Local-agent model selectors currently expose a generic `CLI default` label for most providers, and only pi / oh-my-pi expose discovered models. This hides the model a CLI will actually use and prevents explicit model selection where the installed CLI already has a stable, non-completion listing command.

## Scope

- Keep every polish request as a direct, isolated CLI invocation.
- Use only stable, non-completion CLI commands for discovery; do not depend on experimental servers or protocols.
- Show the current CLI default model when a stable command exposes it.
- List selectable models for providers with a stable non-interactive listing command.
- Use an honest provider-specific default label when the exact model cannot be discovered.
- Preserve empty model IDs as “let the CLI choose” and preserve explicit model overrides.

## Provider contract

| Provider | Stable discovery source | Result |
|---|---|---|
| Claude Code | curated aliases; no stable default query | Claude default + Haiku/Sonnet/Opus |
| pi | RPC `get_state` + `get_available_models` | exact default + available models |
| oh-my-pi | `config get modelRoles --json` + `models --json --no-extensions` | exact default + available models |
| Codex | `doctor --json` | exact default only |
| Droid | `exec --help` Available Models section | exact default + available models |
| Grok | `models` | exact default + available models |
| OpenCode | `models --pure` | available models + provider default |
| Cline | no stable non-interactive list/default command | provider default only |

## Acceptance

1. The local-agent model dropdown renders the backend-provided default label instead of hardcoded `CLI default`.
2. Empty selection still persists an empty model ID.
3. Claude empty selection no longer silently pins Haiku.
4. Installed pi, oh-my-pi, Droid, Grok, and OpenCode expose their available models without making a completion request.
5. Installed Codex shows its current configured/default model.
6. Missing, malformed, oversized, failed, and timed-out discovery commands return the existing typed provider errors without leaving child processes behind.
7. Focused Rust/frontend tests, quality checks, direct installed-CLI smokes, and native model-dropdown smoke pass.
