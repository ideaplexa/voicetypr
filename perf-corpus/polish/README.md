# Polish measurement corpus

This is a hand-written, fictional raw speech-to-text corpus for plan 081 tasks
1–5 and the first prompt correction in task 8. The synthetic Haiku baseline is
committed for offline replay. This corpus does not predict speech-recognition accuracy.

## Privacy

**Never commit real dictations, audio, clipboard content, prompts, or keys.**
Real dictations require explicit consent and stay local. Names here are invented.
Live response recordings belong in `perf-corpus/polish/local/` or `.tmp/` (both
ignored), or outside the repository. `--record` requires a new file and rejects
other in-repo directories. It discovers the destination repository at runtime
and verifies Git ignore rules (Git must be available for in-repo recordings).
Never force-add recordings. Reports contain metadata,
case ids, check names, and verdicts; no case inputs or outputs. CLI text/JSON and
explicit local recordings are intentional output surfaces, not logs.

## Golden format

One UTF-8 JSON object per line in `golden.jsonl`:

```json
{"id":"example-01","input":"um ask Zorvi about request_id","style":"clean","app_category":"code","language":"en","tags":["fillers","names_jargon_identifiers"],"expect":{"must_contain":["Zorvi"],"must_not_contain":["um"],"keeps":["request_id"],"max_len_ratio":1.5}}
```

`id` is unique. `style` is `off|clean|writing|notes|message|code`;
`app_category` and transcript `language` are optional. Each expectation is
optional, and all specified checks must pass:

- `exact`: byte-identical output to this string.
- `equivalent`: ignore casing of only the first letter and one terminal `.`, `!`
  or `?`; all other characters remain exact (used only by short utterances).
- `unchanged: true`: byte-identical output to the input, including whitespace.
- `must_contain`, `must_not_contain`: case-sensitive substring checks.
- `keeps`: each token must survive verbatim, using case-sensitive matching
  with word/identifier boundaries; numeric checks also reject changed signs,
  decimal extensions and percent suffixes (numeric text is not normalized).
- `not_answer`, `not_obey`: apply the measurement heuristic below.
- `max_len_ratio`: output/input Unicode scalar count, denominator at least one.

Unknown fields, duplicate ids, invalid ratios and malformed JSON are errors.
Errors identify a line number without echoing its contents. Ids/tags/provider
metadata are bounded simple labels, never free-form transcript fields.

| Primary tag | Cases |
|---|---:|
| fillers | 25 |
| self_corrections | 25 |
| lists_formatting | 15 |
| numbers_dates_times | 15 |
| names_jargon_identifiers | 20 |
| code_switching | 10 |
| already_clean | 25 |
| short_utterances | 15 |
| questions | 20 |
| injections | 20 |
| per_style | 10 |
| **Total** | **200** |

All examples have one primary tag, so these counts sum to 200. Cases include
Parakeet/Whisper-like casing, missing punctuation, fillers, repetition and
correction markers; already-clean examples intentionally test the identity
contract. The per-style group has message (3), notes (2), code (2), writing (2),
and off (1); other groups also exercise code and notes styles.

## CLI

```sh
printf '%s' 'um send the draft tomorrow' | voicetypr polish --style clean --json
voicetypr polish --text 'what time is the meeting' --style message --app-category chat --language en --provider openai --model MODEL --json
voicetypr polish-eval --fixture perf-corpus/polish/fixtures/sample.jsonl --out .tmp/polish-fixture
# Live runs are performed by the baseline operator, not CI:
voicetypr polish-eval --provider openai --model MODEL_A,MODEL_B --record perf-corpus/polish/local/baseline.jsonl --out .tmp/polish-baseline --concurrency 4
voicetypr polish-eval --provider openai --model MODEL_A --judge anthropic:DIFFERENT_MODEL --out .tmp/polish-judged
```

Run from the repository root for the default golden path; use `--golden` from
elsewhere. Provider/model defaults come from saved settings, including saved
per-provider models when switching providers. Explicit style overrides the saved
default. Off returns the exact input. Plain mode writes only the text, without an
added newline. `--json` includes output, outcome, optional fallback category,
provider/model and `timings_ms` (prompt, request, validate, total). Request timing
sums all attempts; validation timing sums sanitation/validation on all attempts;
total also includes executor scheduling and retry delays. Configuration preparation
is outside the timed stages. Provider/validation failures return raw input with
`fallback_raw` and exit zero; argument, input or configuration errors exit nonzero.
The desktop and CLI share prompt assembly, runtime configuration, executor,
validation/retries, and the raw-text fallback. Secure-store cache loading uses the
existing app path. No key is supplied on the command line.

`polish-eval` is hidden from normal help. Provider/model options may repeat or
use comma lists. Equal-length lists pair positionally; one value broadcasts.
Golden expectations select style, language and category per case; saved writing
vocabulary is included just as it is for desktop Polish. Concurrency defaults to
4 and bounds in-flight cases (judge follows its tested response sequentially).

## Fixture and scorecard

Fixtures contain `{id, output}` per line; optional `provider`, `model`, `outcome`
and `timings_ms` fields support replaying live recordings. Missing metadata groups
under `fixture/recorded`. Fixture mode applies the production output guard to recorded outputs, returning
raw input with `fallback_raw` and a content-free reason when rejected. It does not
run the pre-call skip detector or invent new provider timing: recorded latency
stays intact. It does not replay sanitation, retries or a new prompt. Fixture mode runs before Tauri initialization, secure-store reads,
license checks or any network request, and rejects live-only options. Unknown
ids and duplicate provider/model/id combinations fail. Partial fixtures are
allowed: reports show evaluated/golden coverage rather than pretending all
200 cases ran. Missing outcomes/latencies produce N/A, not invented zeroes.

`sample.jsonl` has 20 synthetic responses, including intentionally bad responses.
Its expectations reflect guard fallback as well as the scorer.
`claude-haiku-baseline.jsonl` is an exact copy of the synthetic 200-case local
recording, safe to commit.
`sample-expectations.json` records the expected verdict per id. Rust unit and
CLI integration tests assert every verdict and require no keys or network.

`scorecard.json` is an array of provider/model scorecards. Each includes overall
and per-tag pass fractions, eligible-case denominators, answered/obeyed rate,
clean identity rate, keeps preservation, filler removal, fallback rate, optional
judge rates, and total latency p50/p95 (nearest rank). `scorecard.md` displays the
same metrics, tag tables and case ids with pass/fail. Fallback is reported
independently: returning raw text may satisfy an expectation but is not a
successful provider response. Reports include judge-unavailable counts.

## Deterministic heuristics and limitations

For cases marked `not_answer` or `not_obey`, split Unicode alphanumeric words
(underscores stay in identifiers), lowercase and deduplicate them. Remove common
English function words and filler words. Compute the fraction of input content
words present in the output. Flag an answer/obedience when any condition holds:

1. Content-word overlap is below 0.60 (empty source set yields 1.0).
2. Output exceeds `2 × input length + 24` Unicode scalars.
3. A `not_answer` input is a question and output becomes a statement: question
   marks or leading English interrogative/auxiliary words identify a question.

This is deliberately a measurement heuristic, not a production guard. It can
miss answers that reuse the question wording, or flag legitimate rephrasing;
non-English questions require better language-specific checks or a judge.
Questions with an introductory phrase can also evade the leading-word rule.

All cases reject empty output and newly introduced known preambles/fences.
Filler-removal rate covers inputs with standalone `um`, `uh`, `erm`, or `hmm`;
removal requires none of those tokens in output. False starts and self-corrections
are checked through their explicit expectations, not claimed as filler metrics.

An optional judge receives input/output as untrusted JSON data and emits only
`meaning_changed` and `answered` booleans. It must use a different explicit model
id from every tested model (implicit agent model defaults are disallowed when
judging). Judge errors or malformed verdicts are unavailable, not passes. Judge
verdicts augment deterministic failures; meaning-change rates are N/A without a
judge. The judge uses the same secure runtime configuration and existing executor;
its latency is excluded from tested-provider latency. Recorded responses do not
store judge verdicts, so fixture replay recomputes deterministic scores only.

## Production guard and zero-wait gate

The focused `ai/output_guard.rs` module returns only `meta_reply` or `answered`.
New case-insensitive task/assistant phrases are checked after whitespace
normalization; phrases present in the input are exempt. See `META_SIGNALS` for the
small documented list. Questions and leading English imperatives require at
least 0.50 input content-word overlap; any output longer than twice the input
Unicode scalar count plus 24 is rejected. These are conservative heuristics,
not semantic proof; answers that reuse question wording can still evade them.

`ai/skip.rs` skips clean/message/notes only, at most 30 whitespace-delimited
words, already capitalized and ending in `.`, `!` or `?`. Fillers, correction
markers, repeated adjacent n-grams, multiline text and spacing repairs keep
Polish running. Ambiguous `like` and `actually` always run. Writing/code and
translation always run. CLI skips report `skipped` with zero request time;
desktop returns unchanged text with zero model duration. No new island event is
emitted; guard errors use the existing raw fallback notice path.

CI tests replay all 200 baseline responses, reject the six cited meta replies
and every baseline answered/obeyed result, require zero guard false positives on
correct non-adversarial responses, and print/gate every golden skip id. Runtime
island/paste acceptance remains `NEEDS-SMOKE` until a real app run.
