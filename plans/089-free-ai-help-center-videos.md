# 089: Free AI routes, help center and help videos

Founder direction, 13 November 2026: make the free Cloudflare allowance close to
one click, and cover every app situation for every kind of user with a help
center and help videos.

Status: plan. Nothing here is built or measured unless a line says so.

## 1. "Connect Cloudflare" (free Polish)

Facts checked on Cloudflare's docs on 13 November:

- 10,000 neurons per day free on every account; a Polish costs about 3 to 4
  neurons (see `docs/reports/2026-11-13-decider-and-small-models-for-polish.md`).
- There is no sign-in button a third-party app can use. Cloudflare offers
  [token template URLs](https://developers.cloudflare.com/fundamentals/api/how-to/account-owned-token-template/)
  that open the "create token" page with the permission already filled in. The
  user must still press Create and copy the token.
- Workers AI has an OpenAI-compatible endpoint, the same shape as the app's
  existing Custom provider (`ai/openai_compatible.rs`).

So the honest shortest flow is two clicks and one paste:

1. User presses "Use free Cloudflare AI" in Polish settings.
2. The browser opens the pre-filled token page; user presses Create, then Copy.
3. User pastes the token into the app. The app finds the account ID itself,
   stores the token in `secure_store`, picks the model and runs a test Polish.

Not verified: the permission key name for Workers AI in a template URL; that a
Workers AI token may list its own account (`GET /accounts`); that the Custom
provider works against the endpoint unchanged.

Gate before building: measure real Polish speed and quality on Workers AI
models with the founder's token on real transcripts. If the best model is
slower than about 1 second or rewrites meaning, do not ship the button.

## 2. Help center

Lives in the `voicetypr-web` repo, `apps/web/content/help/`. It has 15 articles,
last checked in July, all about install, microphone, hotkeys, models, billing
and privacy. Nothing covers Polish, cloud engines, languages, network sharing
or the pill.

First batch (branch `docs/help-2.1`, worktree `worktrees/voicetypr-web-help-2.1`):

| Article | For whom |
|---|---|
| Use Voicetypr without paying for AI | Anyone avoiding another subscription |
| Polish: clean up what you said | Everyone who turns Polish on |
| Cloud speech engines and API keys | Users who want cloud accuracy |
| Languages | Non-English and mixed-language speakers |
| Share a model across your computers | Users with a second machine |
| The recording pill | New users |
| Update "Choose a speech model" | Everyone (Unified, Nemotron, Windows Parakeet missing) |

Rule: an article describes only what the 2.1 code does, with the exact labels
from the UI. Each stays unpublished until its steps are followed once in the
real app. The Cloudflare article waits for section 1.

## 3. Help videos

Decision: short screen recordings of the real app, 30 to 60 seconds, one task
each, captions and no voice. Captions translate; a voice does not, and a silent
clip is cheap to redo when the UI changes. One video per help article, embedded
at the top.

Blocked on: a smoke-tested 2.1 build, because a video of an untested build
teaches the wrong thing.

First five: first run to first dictation; pick a model; turn on Polish with a
subscription you already have; fix "nothing was pasted"; dictate in another
language.
