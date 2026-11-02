# Plan 086: Multilingual text correctness for beta.6

Status: SPEC, not claimed for implementation. Proposed target: 2.1.0-beta.6,
subject to founder approval of an addition to the approved scope in
[plan 083](083-2.1-beta6.md) and re-estimation.
Audit baseline: beta.5 worktree commit
`9581cc6a1e859be5fd3f28e1ec99eefc63fbb406`; check current code and merge
drift before implementation. Changes are independent of opt-in punctuation
commands in [plan 085](085-spoken-punctuation-commands.md), but coordinate
shared formatting and pipeline files. Coordinate shared Polish prompts and
guards with [plan 084](084-local-polish-model.md), whose local model remains a
separate beta after evaluation and is not a dependency of this plan.

Beta.5 already restricts sentence capitalization and clean-text Polish skip to
English, normalizes regional dictionary codes to base language, rejects
invalid codes, and has a dormant German abbreviation list. Do not undo these
safety gates without the evidence below. As plan 083 records, local and remote
CLI transcription reach the shared writing pipeline, including library rules
and optional Polish; test both CLI and desktop paths without claiming complete
parity for app context or cloud diarized early returns.

## Ranked work

1. **P0, L: sentence boundaries and capitals.** Define language-specific,
   conservative boundaries and abbreviation policies in
   `writing/format.rs` / `crates/transcript-text`; enable capitalization one
   language at a time only after fixture and real-speech validation. German:
   `ggf`, `vgl`, `bspw`, `evtl`, `inkl`, `zzgl`, `sog`, `Abs`, `Hr`, `Fr`,
   `Mag`, `lit`, `Art`, `gem.`; specifically `Art.`/`Abs.` before numbers.
   Add French `Mme`, `M.`, Spanish `Sr`/`Sra`/`Ud`, Polish `np`/`itd`/`itp`/
   `tj`/`godz`, Russian `ул`/`им`, Ukrainian `вул`, Dutch `dhr`/`mevr`.
   Recognize German `„“` and `»«`; `„Wirklich?“ fragte er` must retain
   lowercase attribution. Cover Dutch `'s morgens` and titlecase `IJ`, and
   Turkish dotted/dotless i. Unknown language keeps beta.5 conservative
   behavior. No blanket Unicode uppercase heuristic.
2. **P0, M: French spacing.** Stop deleting spaces before `? ! : ;` both in
   Whisper normalization and `crates/transcript-text`. Preserve existing
   U+00A0 and U+202F and ordinary spaces; do not introduce or normalize one
   form until locale policy is approved. Assert exact code points and
   idempotence through both stages.
3. **P0, M: translation intent.** In `writing/pipeline.rs` around 255-270
   and 564-575 and `ai/prompts.rs` around 261-291, recheck line drift. Unknown
   source plus “same as transcript” must not request English translation.
   Thread explicit no-translation versus translate-to-language intent, not
   a guessed source; treat empty or whitespace-only language as missing.
   Explicit target translation must still work when source is unknown.
4. **P1, L: CJK joining and insertion.** Audit Whisper segment joining
   (`whisper/transcriber.rs` around 790-810), Deepgram realtime finals and
   Soniox / diarized token joins under `cloud_stt/`. Avoid invented ASCII
   spaces between Han/kana, preserve real word spacing and boundaries in
   Korean, Latin and mixed script. Fix `commands/text.rs` adding trailing space
   after `!` in Han/kana, full-width punctuation paragraph boundaries, and
   `insertion/leading_space.rs` treating Hangul like Han. Engine-specific
   token metadata and actual separator evidence take precedence over guesses.
5. **P1, M: snippets.** In `writing/library_rules.rs`, replace ASCII-only
   case comparison and ASCII-only trailing `. ! ?` stripping with tested
   Unicode-aware, language-sensitive behavior. Cover `Öffne`, `Привет`,
   `挨拶。` and inverted Spanish `¿`; do not strip internal punctuation or
   turn nonmatches into matches accidentally.
6. **P1, L: dictionary.** Existing alphanumeric boundaries miss replacements
   within Han text; define safe language-specific segmentation and match
   scope, never substring-replace unrelated words. Test Turkish case folding
   and NFC/NFD equivalent forms while preserving unrelated original bytes.
   Keep regional normalization and invalid-code rejection from beta.5.
7. **P1, M: Polish output guard.** `ai/output_guard.rs` uses English lexicons
   and ASCII `?` only. Characterize CJK false accepts and false rejects with
   full-width `？`; replace English-only assumptions with conservative,
   language-aware rules. On uncertainty retain safe fallback to original,
   never accept an answer to a dictated question as cleaned text. Measure
   both false accept and false reject against labeled fixtures.
8. **P1, M: other scripts and security.** Arabic `؟` and Hindi `।` are
   sentence ends. AI output currently strips RTL marks: inventory exact code
   points, legitimate bidi formatting and spoofing/injection risk before
   changing this. Require security review and explicit allow/strip policy,
   tests for isolates/overrides and safe display; do not blindly preserve
   all bidi controls. Test Hebrew and Arabic mixed-script text.
9. **P2, S: protected lowercase terms.** Language-filter protected lowercase
   terms before applying them; test identical terms in different languages,
   unknown language and user vocabulary. Do not alter proper names or words
   outside that language.
10. **P2, L: language provenance.** Cloud results inherit requested language,
    and a take currently has one language even when speech is mixed. Model
    requested, provider-detected and unknown provenance separately; do not
    label a request as detection. Carry per-segment evidence only where
    provider supplies it, and use conservative behavior on conflict or
    code-switching. No fabricated detection, no new telemetry of text.

## Fixture contract and minimum coverage

Use table-driven cases with fields `id`, `input`, `language`, `engine`,
`flags`, `expected`; expected is exact Unicode text or a typed expected
prompt/guard decision. Provide separate fixture sets for **insertion**,
**dictionary**, **Polish prompt**, and **guards**, plus formatting/snippets
and segment joining. Use synthetic text examples, not real private transcripts.
Every group below is a minimum, not a substitute for edge and negative cases.

| Group | Minimum cases and exact assertions |
|---|---|
| German | One positive and one negative boundary case for each abbreviation in item 1; `Art. 5` and `Abs. 3`; `3,5`, `3. Oktober`, `1.11.2026`, `§`; both quote pairs and lowercase attribution; repeat with `de`, `de-AT`, `de-CH`; `Straße` and `Strasse` remain distinct and untouched absent an explicit rule. |
| French | Each of `? ! : ;` with ASCII space, U+00A0 and U+202F, through normalizer and shared crate; `Mme` and `M.` in context. Assert exact preservation. |
| Spanish / Portuguese / Italian | At least one sentence, question and abbreviation or boundary case each; Spanish `Sr`, `Sra`, `Ud`, and `¿` snippet. Assert punctuation and word preservation. |
| Dutch | `dhr`, `mevr`, `'s morgens` and `IJ` titlecasing in sentence and non-sentence contexts. |
| Polish / Turkish / Cyrillic | Each Polish abbreviation in item 1; Turkish `i`, `İ`, `ı`, `I` in sentence and dictionary matches; Russian `ул`, `им` and Ukrainian `вул`; Cyrillic snippet `Привет`. Include nonmatches. |
| Arabic / Hebrew / Hindi | Arabic `؟`, Hindi `।`, Hebrew/Arabic RTL text and mixed Latin with explicit safe and unsafe bidi controls; assert security-reviewed code-point output and guards. |
| Chinese / Japanese / Korean | Han and kana adjacent segments/finals, full-width `。！？`, snippet `挨拶。`, question guard and insertion after `!`; Korean spaced words and cursor helper; mixed Latin/CJK and intentional spaces. |
| Metadata / pipeline | Missing, empty, whitespace and regional codes; unknown source + same-language versus explicit target; requested versus detected provenance, conflicting/mixed segments; local and cloud, Polish on/off, preview/final and failure fallback. |
| Invariants | Idempotence for each transform; never empty a nonempty transcript; no lost words except eligible English cleanup; no newly added Han/kana spaces, no loss of intentional Korean spaces, no cross-language lowercase-term leak. Include composed/decomposed Unicode and insertion boundaries. |

Run each relevant case at the unit seam and end-to-end through the shared
production pipeline. Engine values include Whisper, Deepgram realtime, Soniox,
and a no-provider fixture; include diarization and final/preview flags where
relevant. Test insert/paste on both desktop platforms; no simulated Enter
that might submit a message. Golden assertions must distinguish byte equality
from explicitly approved Unicode equivalence.

## Phases and gates

1. **S: baseline and reproduce.** Pin beta.5 drift, inventory callsites, add
   separate labeled fixture sets and record failures without logging text.
   Establish false-accept and false-reject guard baselines.
2. **L: P0.** Ship intent fix and French preservation first. Implement
   sentence policies behind per-language gates, initially preserving beta.5
   defaults; enable only the languages whose boundary tests and consented
   real-speech smoke pass. No dependency on plan 085.
3. **L: P1.** Segment joining, snippets, dictionary, guard and RTL work in
   reviewable slices. Security review is a hard gate on any bidi behavior
   change. Test provider fallback and raw-text preservation.
4. **L: P2 and release.** Language-filter lowercase terms; introduce
   provenance only with explicit unknown/mixed behavior. Re-run full fixtures,
   integration and native insertion, compare guard regressions to baseline.
   If a slice is not ready, keep its existing conservative gate and record
   it as deferred rather than claiming all-language support.

Acceptance: P0 fixes pass all specified fixtures and preserve existing beta.5
gates; each enabled language passes relevant cases, no word loss and no
unintended translation; P1/P2 changes ship only when their own tests and
security gates pass. Before releasing an included slice, exercise consented real speech across
the affected language groups and available local/cloud engines, with German
and French as priorities, and native macOS/Windows insertion. Keep
NEEDS-SMOKE for missing languages, providers or hardware and report them
explicitly. Never judge engine accuracy on synthetic TTS. No transcript, audio,
clipboard, prompt, path or window title in logs or analytics.

## Founder decisions

- Which languages must pass real-speech and hardware smoke for beta.6, and
  which may stay behind conservative gates until later?
- Approve adding this scope to beta.6 and a revised estimate, or assign the
  work to a later beta without delaying the approved plan 083 release?
- Should French typography preserve user/engine whitespace indefinitely or
  normalize to U+00A0 or U+202F by locale? What is the de-CH quote policy?
- Is provider language detection trustworthy enough for automatic formatting,
  or should requested/detected disagreement always disable language-specific
  transformations? Approve the security-reviewed RTL policy before release.
