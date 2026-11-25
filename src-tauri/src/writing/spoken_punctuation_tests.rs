use super::apply_spoken_punctuation;

fn convert(text: &str, language: &str) -> String {
    apply_spoken_punctuation(text, Some(language)).unwrap_or_else(|| text.to_string())
}

fn de(text: &str) -> String {
    convert(text, "de-AT")
}

/// What the customer wanted (24 Nov). Voicetypr keeps the recognizer's case,
/// so a word Whisper capitalised after its own full stop stays capitalised.
const INTENDED: &str = "Ich teste jetzt noch einmal, wie gut das mit den Satzzeichen \
funktioniert; bin schon gespannt, ob auch „Höchstbetragshypothek“ richtig geschrieben wird. \
Das ist in § 131 ABGB geregelt. Ich bin auch gespannt, ob die Software bei diesem \
(kurz diktierten) Text schon weitgehend fehlerfrei arbeitet!";

#[test]
fn customer_dictation_with_whisper_full_stop_after_strich_punkt() {
    // A raw Whisper rendering that produced exactly what the customer got on
    // 2.0.12: "Strichpunkt" split in two, pause commas around both quote
    // commands, and "Klammer" heard as "Glammer".
    let raw = "Ich teste jetzt noch einmal, wie gut das mit den Satzzeichen funktioniert, \
Strich Punkt. Bin schon gespannt, ob auch, Anführungszeichen unten, Höchstbetragshypothek, \
Anführungszeichen oben, richtig geschrieben wird. Das ist in § 131 ABGB geregelt. Ich bin \
auch gespannt, ob die Software bei diesem Glammer auf, kurz diktierten Glammer zu, Text \
schon weitgehend fehlerfrei arbeitet, Ausrufezeichen.";
    assert_eq!(de(raw), INTENDED.replace("; bin", "; Bin"));
}

#[test]
fn customer_dictation_with_whisper_commas_comes_out_as_intended() {
    let raw = "Ich teste jetzt noch einmal, wie gut das mit den Satzzeichen funktioniert \
Strich-Punkt, bin schon gespannt, ob auch Anführungszeichen auf, Höchstbetragshypothek, \
Anführungszeichen zu, richtig geschrieben wird. Das ist in § 131 ABGB geregelt. Ich bin \
auch gespannt, ob die Software bei diesem, Glammer auf, kurz diktierten, Glammer zu, Text \
schon weitgehend fehlerfrei arbeitet Ausrufezeichen";
    assert_eq!(de(raw), INTENDED);
}

#[test]
fn customer_dictation_with_unsplit_commands_comes_out_as_intended() {
    let raw = "Ich teste jetzt noch einmal, wie gut das mit den Satzzeichen funktioniert \
Strichpunkt bin schon gespannt, ob auch, Ausführungszeichen unten, Höchstbetragshypothek \
Ausführungszeichen oben richtig geschrieben wird. Das ist in § 131 ABGB geregelt. Ich bin \
auch gespannt, ob die Software bei diesem Klammer auf kurz diktierten Klammer zu Text \
schon weitgehend fehlerfrei arbeitet. Ausrufezeichen";
    assert_eq!(de(raw), INTENDED);
}

#[test]
fn split_strichpunkt_becomes_a_semicolon() {
    for raw in [
        "erstens Strich Punkt zweitens",
        "erstens Strich-Punkt zweitens",
        "erstens strich punkt zweitens",
        "erstens. Strich Punkt. zweitens",
    ] {
        assert_eq!(de(raw), "erstens; zweitens", "{raw}");
    }
}

#[test]
fn austrian_glammer_is_a_bracket() {
    assert_eq!(de("siehe Glammer auf Anlage Glammer zu"), "siehe (Anlage)");
    assert_eq!(
        de("Fußnote eckige Glammer auf 1 eckige Glammer zu"),
        "Fußnote [1]"
    );
    assert_eq!(
        de("Menge geschweifte Glammer auf a geschweifte Glammer zu"),
        "Menge {a}"
    );
}

#[test]
fn ordinary_strich_punkt_klammer_and_komma_stay_words() {
    for prose in [
        "Unter dem Strich bleibt ein Gewinn.",
        "Ein Strich und ein Punkt ergeben das Zeichen.",
        "Der springende Punkt ist die Frist.",
        "Die Klammer hält die Seiten zusammen.",
        "Die Glammer ist kein Wort.",
        "Hier fehlt ein Komma.",
        "Der Zinssatz beträgt 3 Komma 5 Prozent.",
        "Die Strichpunkte und Doppelpunkt-Regeln stehen im Duden.",
        "Die Klammer aufheben, bitte.",
        "Er zog einen Strich, Punkt, dann noch einen.",
    ] {
        assert_eq!(de(prose), prose);
    }
}

#[test]
fn a_period_after_a_word_ending_in_strich_still_converts() {
    assert_eq!(
        de("Am Ende steht ein Querstrich Punkt"),
        "Am Ende steht ein Querstrich."
    );
    assert_eq!(de("Er zog den Strich Befehl Punkt"), "Er zog den Strich.");
    assert_eq!(
        de("Er zeichnete einen Strich Punkt"),
        "Er zeichnete einen Strich."
    );
    assert_eq!(
        de("Das gilt unter dem Strich, Punkt."),
        "Das gilt unter dem Strich."
    );
    assert_eq!(
        de("Das lohnt sich unter dem Strich Satzendpunkt"),
        "Das lohnt sich unter dem Strich."
    );
}

#[test]
fn german_triggers_keep_working() {
    assert_eq!(de("Das ist der Vertrag Punkt"), "Das ist der Vertrag.");
    assert_eq!(de("Das ist der Vertrag Punkt."), "Das ist der Vertrag.");
    assert_eq!(de("3 Komma 5"), "3 Komma 5");
    assert_eq!(de("3 Befehl Komma 5"), "3, 5");
    assert_eq!(
        de("Wir brauchen eine neue Zeile im Formular."),
        "Wir brauchen eine neue Zeile im Formular."
    );
    assert_eq!(
        de("Das Wort wörtlich Komma bleibt"),
        "Das Wort Komma bleibt"
    );
    assert_eq!(de("Befehl Raute eins"), "#eins");
    assert_eq!(de("Raute eins"), "Raute eins");
}

#[test]
fn recognizer_commas_around_brackets_and_quotes_go_in_every_language() {
    let cases = [
        (
            "en",
            "the fee, open paren, net, close paren, is due",
            "the fee (net) is due",
        ),
        (
            "en",
            "she wrote, open quote, done, close quote, and left",
            "she wrote “done” and left",
        ),
        (
            "de",
            "Die Gebühr, Klammer auf, netto, Klammer zu, ist fällig",
            "Die Gebühr (netto) ist fällig",
        ),
        (
            "de",
            "Er schrieb, Anführungszeichen unten, erledigt, Anführungszeichen oben, und ging",
            "Er schrieb „erledigt“ und ging",
        ),
        (
            "fr",
            "les frais, ouvrez la parenthèse, nets, fermez la parenthèse, sont dus",
            "les frais (nets) sont dus",
        ),
        (
            "fr",
            "il a écrit, ouvrez les guillemets, fini, fermez les guillemets, et il est parti",
            "il a écrit «\u{A0}fini\u{A0}» et il est parti",
        ),
        (
            "es",
            "la tarifa, abrir paréntesis, neta, cerrar paréntesis, vence hoy",
            "la tarifa (neta) vence hoy",
        ),
        (
            "es",
            "escribió, abrir comillas, listo, cerrar comillas, y se fue",
            "escribió «listo» y se fue",
        ),
        (
            "it",
            "la tariffa, apri parentesi, netta, chiudi parentesi, scade oggi",
            "la tariffa (netta) scade oggi",
        ),
        (
            "it",
            "ha scritto, apri virgolette, fatto, chiudi virgolette, e se ne andò",
            "ha scritto “fatto” e se ne andò",
        ),
        (
            "pt",
            "a taxa, abre parênteses, líquida, fecha parênteses, vence hoje",
            "a taxa (líquida) vence hoje",
        ),
        (
            "pt",
            "escreveu, abre aspas, pronto, fecha aspas, e saiu",
            "escreveu “pronto” e saiu",
        ),
        (
            "nl",
            "het tarief, haakje openen, netto, haakje sluiten, is verschuldigd",
            "het tarief (netto) is verschuldigd",
        ),
        (
            "nl",
            "hij schreef, aanhalingsteken openen, klaar, aanhalingsteken sluiten, en ging",
            "hij schreef “klaar” en ging",
        ),
        (
            "pl",
            "opłata, otwórz nawias, netto, zamknij nawias, jest należna",
            "opłata (netto) jest należna",
        ),
        (
            "pl",
            "napisał, otwórz cudzysłów, gotowe, zamknij cudzysłów, i wyszedł",
            "napisał „gotowe” i wyszedł",
        ),
    ];
    for (language, raw, expected) in cases {
        assert_eq!(convert(raw, language), expected, "{language}: {raw}");
    }
}

#[test]
fn a_sentence_end_after_a_closing_symbol_stays() {
    assert_eq!(
        de("siehe Klammer auf Anlage Klammer zu. Danach"),
        "siehe (Anlage). Danach"
    );
    assert_eq!(
        convert("she said open quote no close quote?", "en"),
        "she said “no”?"
    );
}

#[test]
fn a_dictated_comma_next_to_a_quote_stays() {
    assert_eq!(
        de("Anführungszeichen unten, Erledigt, Anführungszeichen oben, Komma, sagte er"),
        "„Erledigt“, sagte er"
    );
    assert_eq!(
        de("Anführungszeichen unten Erledigt Anführungszeichen oben Komma sagte er"),
        "„Erledigt“, sagte er"
    );
    assert_eq!(
        convert("he said comma, open quote, hello, close quote.", "en"),
        "he said, “hello”."
    );
    assert_eq!(
        convert("open quote go comma close quote he said", "en"),
        "“go,” he said"
    );
}

#[test]
fn spanish_keeps_the_comma_before_an_opening_question_mark() {
    assert_eq!(
        convert("Oye, abrir interrogación vienes cerrar interrogación", "es"),
        "Oye, ¿vienes?"
    );
}

#[test]
fn recognizer_marks_around_other_symbols_go() {
    assert_eq!(de("funktioniert, Strichpunkt. Bin"), "funktioniert; Bin");
    assert_eq!(
        de("Höchstbetrags, Bindestrich, Hypothek"),
        "Höchstbetrags-Hypothek"
    );
    assert_eq!(
        de("um 10, Prozentzeichen gestiegen"),
        "um 10\u{A0}% gestiegen"
    );
    assert_eq!(
        de("geregelt, Gedankenstrich, und zwar"),
        "geregelt – und zwar"
    );
    assert_eq!(de("gemäß, Paragraphzeichen, 131"), "gemäß §\u{A0}131");
}

#[test]
fn line_and_paragraph_breaks_with_whisper_marks() {
    assert_eq!(
        de("Das ist geregelt. Neuer Absatz. Ich bin gespannt"),
        "Das ist geregelt.\n\nIch bin gespannt"
    );
    assert_eq!(
        de("Das ist geregelt Punkt neuer Absatz Ich bin gespannt"),
        "Das ist geregelt.\n\nIch bin gespannt"
    );
    assert_eq!(
        de("Das ist geregelt, Punkt, neuer Absatz, ich bin gespannt"),
        "Das ist geregelt.\n\nich bin gespannt"
    );
    assert_eq!(
        de("Das ist geregelt Punkt. Neue Zeile. Ich bin gespannt"),
        "Das ist geregelt.\nIch bin gespannt"
    );
    // A comma before a break is kept: it is the salutation comma of a letter.
    assert_eq!(
        de("Sehr geehrte Damen und Herren, neuer Absatz, ich schreibe"),
        "Sehr geehrte Damen und Herren,\n\nich schreibe"
    );
    assert_eq!(
        de("Sehr geehrte Damen und Herren Komma neue Zeile ich schreibe"),
        "Sehr geehrte Damen und Herren,\nich schreibe"
    );
    assert_eq!(
        de("Hier beginnt ein neuer Absatz."),
        "Hier beginnt ein neuer Absatz."
    );
}
