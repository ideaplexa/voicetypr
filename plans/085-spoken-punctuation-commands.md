# Plan 085: Spoken punctuation commands for 2.1

Status: SPEC, not claimed for implementation. Target: 2.1, opt-in and off by
default. Code reference: beta.5 worktree at commit
`9581cc6a1e859be5fd3f28e1ec99eefc63fbb406`; recheck its state before coding.
[Plan 086](086-multilingual-text-correctness.md) owns general multilingual
text correctness. This proposed addition is not part of the approved beta.6
scope in [plan 083](083-2.1-beta6.md). The local Polish model in
[plan 084](084-local-polish-model.md) remains a separate beta after evaluation
and is not required for this offline feature.

## Why and decision gate

An Austrian Windows customer uses local Whisper large-v3-turbo for confidential
German legal documents, with Polish off. Spoken punctuation currently appears
as words. This must work without a network, cloud recognition or AI, as well as
with cloud engines selected by other users. Do not collect their documents.

The master plan puts voice commands in 2.2 and says commands have their own
trigger, never guessed from dictation. A setting alone is not that trigger.
This proposal moves a narrow punctuation-only feature into 2.1. **Do not
implement or ship until the founder approves both the earlier scope and the
trigger and ambiguity policy.** It does not authorize editing commands, app
actions, or Command Mode.

## Proposed contract

- Add a persistent setting, off for new and existing users; update Rust
  settings and TypeScript types together. User selects German or English
  command table, including German Austrian aliases; regional codes resolve to
  base language. Unsupported or unknown language does not silently use English.
  More languages require separate reviewed tables.
- Require an explicit spoken prefix, proposed `Befehl` / `command`, immediately
  before each command. Match whole tokens, case-insensitively, longest phrase
  first. An explicit escape, proposed `wörtlich` / `literal`, emits the next
  command phrase as words, without the escape token. A lone, incomplete or
  unknown prefix or escape stays literal. Exact words and localized UI need
  approval. Explain the syntax and show examples in Settings.
- A prefix grants only this single conversion, not a mode that persists.
  Without it, `der springende Punkt`, `ich komme gleich`, prose `Plus`/`Minus`
  and `drei Komma fünf` stay words. Unprefixed recognition, if ever desired,
  is a separate per-command opt-in, off by default and out of this initial
  slice. No position heuristic can guarantee intent. Repeated punctuation
  requires repeated prefixed commands. Preserve unrecognized input.
- `Anführungszeichen` and its observed mishearing `Ausführungszeichen` use
  opening/closing role within a take; explicit `Anführungszeichen auf/zu` is
  preferred. Start closed, toggle only on a recognized command, support
  explicit open/close and preserve an unmatched close literally. Do not guess
  nested roles. English `open quote`/`close quote` are explicit. Ambiguous
  role and a quote spanning takes need founder review.

### Initial command tables

These are phrases after the prefix; output is exact before surrounding spacing.

| German, including Austrian | English | Output |
|---|---|---|
| Beistrich, Komma | comma | `,` |
| Punkt | period, full stop | `.` |
| Strichpunkt | semicolon | `;` |
| Doppelpunkt | colon | `:` |
| Gedankenstrich | dash, en dash | ` – ` |
| neuer Absatz | new paragraph | blank line (`\n\n`) |
| neue Zeile | new line | line break (`\n`) |
| Anführungszeichen, Ausführungszeichen; auf/zu | open quote, close quote | German `„` / `“`; English `“` / `”` |
| Fragezeichen, Ausrufezeichen | question mark, exclamation mark | `?`, `!` |
| Klammer auf/zu, eckige Klammer auf/zu | open/close parenthesis, open/close bracket | `(`, `)`, `[`, `]` |
| Prozent, Paragraph, Und-Zeichen, Raute | percent, section sign, ampersand, hash | `%`, `§`, `&`, `#` |
| Plus, Minus, gleich | plus, minus, equals | `+`, `-`, `=` |
| Schrägstrich, Backslash, Sternchen | slash, backslash, asterisk | `/`, `\`, `*` |

`Doppelpunkt` already supplies `:`. List and tests must cover all requested
symbols, not just the examples above. Do not use German quote shapes in
English, or infer locale-specific quote shapes for de-CH without a decision.

## Pipeline and behavior

1. At the shared final text boundary, after recognition and before optional
   Polish, parse into text and typed punctuation spans. Map the actual beta.5
   paths in `writing/pipeline.rs`, `writing/format.rs` and
   `writing/library_rules.rs` before editing. Both local and cloud finals must
   use the same stage. Verify snippet precedence and prevent a command from
   being interpreted again by Corrections. Local and remote CLI transcription
   use the shared writing pipeline, including library rules and optional Polish
   (plan 083); test this stage there as well, but do not claim complete
   desktop parity, particularly for app context or diarized early returns.
2. Render typed spans deterministically with Polish off. With Polish on,
   protect command spans across the AI call and restore exactly once; if
   protection fails or the model alters placeholders, fall back to the
   deterministic pre-Polish text, never lose dictated words. Do not send
   confidential input to AI unless the user enabled it. Ensure guards evaluate
   the actual resulting text. Coordinate with plan 086's Polish changes and
   preserve plan 084's local-model fallback and evaluation gates.
3. Recognizers may already emit symbols. Whisper's `suppress_nst` suppresses
   some punctuation tokens but does not guarantee none; `Befehl Komma,` and
   `Befehl Punkt.` each become one symbol, not two. Consume only one adjacent
   matching recognizer-emitted mark, never unrelated or repeated literal
   punctuation. Test model variants rather than altering suppression blindly.
4. Format command boundaries, not unrelated text: no space before `, . ; : ? !`;
   closing quotes and brackets attach to their contents, opening ones attach to
   the following word; normal word separation resumes outside them. Dash has
   one space on each side. German `10 %` uses one nonbreaking space (U+00A0)
   as the proposed policy; do not rewrite existing ordinary spaces elsewhere.
   Arithmetic and other symbols need explicit contextual tests, not global
   replacement. Paragraphs have exactly two line feeds, lines one.
5. Sentence-ending command sets a pending sentence-start flag for the next
   word. English can use its tested capitalization policy. Beta.5 limits
   automatic capitalization to English: in German preserve the recognized
   case, even after a spoken `Punkt`, until plan 086 enables a tested German
   sentence policy. No blanket uppercase, including quote attributions.
6. Preview and final must apply the same command policy. Hold any incomplete
   prefix, phrase or boundary in the revisable tail, never rewrite committed
   preview text; if the current streaming contract cannot do that, show the
   unconverted draft and convert only at final, clearly indicating the draft.
   Reject stale sessions/revisions as today. Reconcile final against draft in
   tests, without changing the append-only committed-text invariant.

Existing Corrections are case-insensitive whole-word matches with literal
output. They are not a substitute: their UI cannot enter line breaks and they
leave spaces before symbols. Add multiline editing, storage round-trip and
safe literal output for Corrections as a separate phase, without silently
changing existing correction matching or whitespace. Command spacing is handled
by the typed renderer, not general correction replacement.

Newlines must be inserted as text on macOS and Windows, through a verified
clipboard paste path, not synthesized Enter presses that can submit chat.
Preserve existing clipboard restoration and insertion-failure recovery.
Where safe paste is unavailable, keep the text recoverable and report failure
instead of typing Enter or dropping the newline.

## Phases and acceptance

1. **S: decision and fixtures.** Resolve trigger conflict, language/quote and
   spacing choices. Capture baseline for both engines and insertion paths.
2. **M: parser and renderer.** Table-driven German/Austrian and English tests
   cover every alias and symbol, prefixed/unprefixed and escaped ambiguity,
   case, boundaries, quotes, punctuation already emitted, formatting, sentence
   case and unchanged disabled behavior. Reapplying output is idempotent,
   including literal escaped text and consecutive marks; preserve all words
   except an explicit command or its escape. Round-trip settings, unknown
   language and per-take state.
3. **M: integration.** Local/cloud, Polish off/on, failed placeholder
   protection, snippet/Corrections order, stream chunks splitting commands,
   stale revisions, and final/preview tests. No change to default output.
4. **M: Corrections and delivery.** Multiline UI and persistence tests,
   macOS/Windows paste with lines and paragraphs in a document and a chat
   draft, no accidental send, clipboard restoration and fallback recovery.
5. **S: real-speech smoke.** With consent, test real German Austrian and
   English speech, especially Whisper large-v3-turbo on Windows, Polish off,
   and a cloud path where available. Include commands, ambiguity and duplicate
   marks; record pass/fail and aggregate counts only. Never judge recognition
   accuracy on synthetic TTS. Automated success does not replace native
   hardware smoke: keep NEEDS-SMOKE until it passes.

No transcript, audio, clipboard, prompt or sensitive path in logs or analytics.
Acceptance requires all phase tests, no default regression, and the native
smoke above; report missing provider/hardware as a blocker, not a pass.

## Founder questions

1. Approve 2.1 as a narrow exception to the 2.2 voice-command roadmap and the
   proposed explicit per-command prefix and escape words?
2. Approve the quote role policy and mishearing alias, or require explicit
   open/close only? What should de-CH quotes be?
3. Approve U+00A0 for German percentages and command capitalization remaining
   unchanged in German until plan 086?
4. Is a separately enabled unprefixed mode ever acceptable for legal documents,
   given unavoidable literal-word ambiguity?
