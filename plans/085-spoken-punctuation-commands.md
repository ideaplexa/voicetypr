# Plan 085: Spoken punctuation commands for 2.1

Status: SPEC, not claimed for implementation. Target: 2.1, opt-in and off by
default. Code reference: beta.5 worktree at commit
`9581cc6a1e859be5fd3f28e1ec99eefc63fbb406`; recheck its state before coding.
[Plan 086](086-multilingual-text-correctness.md) owns general multilingual
text correctness. This proposed addition is not part of the approved beta.6
scope in [plan 083](083-2.1-beta6.md). The local Polish model in
[plan 084](084-local-polish-model.md) remains a separate beta after evaluation
and is not required for this offline feature.

## Why and founder decision

An Austrian Windows customer uses local Whisper large-v3-turbo for confidential
German legal documents, with Polish off. Spoken punctuation currently appears
as words. This must work without a network, cloud recognition or AI, as well as
with cloud engines selected by other users. Do not collect their documents.

Soniox v6 now reliably emits spoken commands such as `period`, `new line`
and `scratch that` as words, with no API flag to change this. This plan should
handle the supported punctuation and line commands when enabled; `scratch that`
is an editing command and remains out of scope. V6 also writes spoken prices
as words, such as "15 dollars and 20 cents". A price-formatting rule is a
possible later cleanup, not part of this plan now. See plan 083 for the v6
migration.

On 2026-11-02 the founder approved a narrow punctuation-only exception for
2.1, off by default, with the Dragon-style trigger policy below. This overrides
the master plan's 2.2 timing and separate-trigger rule for this feature only.
It does not authorize editing commands, app actions, or Command Mode.

## Contract

- Add a persistent setting, off for new and existing users; update Rust
  settings and TypeScript types together. User selects German or English
  command table, including German Austrian aliases; regional codes resolve to
  base language. Unsupported or unknown language does not silently use English.
  More languages require separate reviewed tables.
- Match whole tokens case-insensitively, longest phrase first. Unambiguous
  commands work without a prefix. Ambiguous words convert only at a verified
  phrase end, defined below, or immediately after `Befehl` / `command`.
  A prefix applies to one command only. `wörtlich` / `literal` forces the next
  command phrase to remain words, consuming only the escape. A lone,
  incomplete or unknown prefix or escape stays literal. Explain all of this
  in Settings, including that boundary guesses can still mistake intent.
- Bare `Anführungszeichen` and observed mishearing `Ausführungszeichen` are
  ambiguous: apply the same boundary or prefix rule. Proposed role behavior
  starts closed, toggles within a take, and preserves an unmatched explicit
  close literally. Explicit `auf/zu` and English open/close are unambiguous.
  Role, mishearing, nested or cross-take quotes, and de-CH shape remain open.

### Phrase end and conservative matching

For an ambiguous candidate, phrase end means the candidate is followed by
end of the final take, an independently recognized next command, or an
engine-emitted sentence-ending `.`, `?`, `!`, `。`, `？` or `！`. A next
command must itself qualify without treating the candidate as its boundary:
resolve chains from right to left, so a speculative ambiguous word cannot
make another speculative word qualify. A matched, already emitted punctuation
mark is deduplicated as below, but a comma alone is not proof of phrase end.

Where reliable word timestamps survive to the parser, a measured silence of
at least 500 ms between the candidate's end and the next word is a proposed
initial pause threshold, to calibrate on consented real speech before release.
Whisper segment boundaries are not automatically pauses: count one only if
reliable timing shows the same gap. If timestamps are missing, invalid or
not carried through a provider, do not infer pauses from segment/final chunk
boundaries or text whitespace; use only the textual boundary rules above or
the explicit prefix. Confirm which engines actually expose timing end-to-end
before promising pause support.

Protect decimal contexts: `drei Komma fünf` and `three point five` remain
literal even if there is a timing gap; a prefix explicitly overrides this
protection. Likewise `der springende Punkt ist` and `ich komme gleich wieder`
remain words without a qualifying boundary. A noun at the end of a take,
such as `der springende Punkt`, can still be mistaken for a command under the
approved policy: `wörtlich Punkt` forces the literal. Do not claim perfect
intent detection.

### Initial command tables

Output is exact before surrounding spacing. A/U means ambiguous/unambiguous.
Explicit descriptive names and directional phrases are U; short ordinary nouns,
adverbs and arithmetic words are A because they often occur in prose or numbers.

| Output | German, including Austrian | English |
|---|---|---|
| `,` | U: Beistrich; A: Komma | A: comma |
| `.` | A: Punkt | A: period, point, dot, full stop |
| `;` | U: Strichpunkt | U: semicolon |
| `:` | U: Doppelpunkt | U: colon |
| ` – ` | U: Gedankenstrich; A: Strich | U: en dash; A: dash |
| `\n\n`, `\n` | U: neuer Absatz, neue Zeile | U: new paragraph, new line |
| `„` / `“` in German, `“` / `”` in English | U: Anführungszeichen auf/zu; A: Anführungszeichen, Ausführungszeichen | U: open quote, close quote |
| `?`, `!` | U: Fragezeichen, Ausrufezeichen | U: question mark, exclamation mark, exclamation point |
| `(`, `)`, `[`, `]` | U: Klammer auf/zu, eckige Klammer auf/zu | U: open/close parenthesis, open/close paren, open/close bracket |
| `%`, `§`, `&`, `#` | U: Paragraphzeichen, Und-Zeichen, Raute; A: Prozent, Paragraph | U: section sign, ampersand; A: percent, hash |
| `+`, `-`, `=` | A: Plus, Minus, gleich | A: plus, minus, equals |
| `/`, `\`, `*` | U: Schrägstrich, Backslash, Sternchen; A: Stern | U: backslash, asterisk; A: slash, star |

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
   `Punkt.` at a valid boundary each become one symbol, not two. An adjacent
   matching mark alone does not qualify an ambiguous candidate unless it is
   also a sentence-ending mark. Consume only one adjacent matching mark,
   never unrelated or repeated literal punctuation. Test model variants
   rather than altering suppression blindly.
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

1. **S: fixtures.** Record the founder's trigger decision; resolve remaining
   quote and spacing choices. Capture baseline for both engines and insertion
   paths and characterize available timing metadata.
2. **M: parser and renderer.** Table-driven German/Austrian and English tests
   cover every alias and symbol, prefixed/unprefixed and escaped ambiguity,
   case, boundaries, quotes, punctuation already emitted, formatting, sentence
   case and unchanged disabled behavior. Cover every item in Peter's list and
   each ambiguous word at mid-phrase, take end, before a command, before
   sentence punctuation, with prefix and escape, and around the pause threshold
   with and without timing. Test numbers and decimals, false positives,
   Whisper segments without a real pause and command chains. Formatting typed
   spans is idempotent; parse each recognition result only once. Plain rendered
   text cannot safely be reparsed: escaped literal output can itself resemble
   a command. Preserve all words except a recognized command or its escape.
   Round-trip settings, unknown language and per-take state.
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

1. Approve the quote role policy and mishearing alias, or require explicit
   open/close only? What should de-CH quotes be?
2. Approve U+00A0 for German percentages?
