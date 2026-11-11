//! Opt-in spoken punctuation commands.
//!
//! Recognizers write a dictated command such as "semicolon" or "Strichpunkt"
//! as a word. When the user enables this stage, whole-word commands for the
//! transcript language become their symbols before Corrections and Polish run,
//! so it works offline with Polish off.
//!
//! Each command has a trigger. Names that are not ordinary prose always
//! convert. Comma words convert unless they sit between two digits ("3 Komma
//! 5"). Period words ("period", "Punkt", "point") convert only at a phrase end:
//! the end of the dictation, before a recognizer-added `. ? !`, a line break,
//! or another unambiguous command. Words that are mostly prose ("colon",
//! "dos puntos") convert only after the language's prefix ("command colon").
//! A command right after an article stays a word ("a new line of products").
//! The prefix forces any command; the escape word ("literal period") keeps the
//! next command as a word.
//!
//! Command names follow the Dragon, Microsoft and Apple dictation lists for
//! each language; typography follows each language's main reference (Duden,
//! Imprimerie nationale, RAE, ...). Transcript languages carry no region, so
//! regional styles (Swiss German, Québec French) use the main default.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};
use serde::Serialize;

/// How a symbol joins the surrounding words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// `, ; : ? ! . …` attach to the previous word, replacing a mark the
    /// recognizer put next to the command; one space follows.
    AfterWord,
    /// `% *` attach to the previous word; surrounding marks stay.
    Suffix,
    /// Line or paragraph break; the next word starts the new line.
    Break,
    /// `( [ { „ “ « ¿ ¡ # §` attach to the next word.
    Opening,
    /// `) ] } “ ” »` attach to the previous word.
    Closing,
    /// `– — & + - =` stand alone with one space on each side.
    Spaced,
    /// `- / \ — –` join the neighbouring words without spaces.
    Joining,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpokenPunctuationTrigger {
    Always,
    /// Comma words: stay literal between two digits ("3 Komma 5").
    NotBetweenDigits,
    /// Period words: only at a phrase end.
    PhraseEnd,
    /// Mostly-prose words: only right after the language's prefix.
    PrefixOnly,
}

use SpokenPunctuationTrigger as Trigger;

struct Command {
    phrases: &'static [&'static str],
    symbol: &'static str,
    placement: Placement,
    trigger: Trigger,
}

const fn always(
    phrases: &'static [&'static str],
    symbol: &'static str,
    placement: Placement,
) -> Command {
    Command {
        phrases,
        symbol,
        placement,
        trigger: Trigger::Always,
    }
}

const fn comma(phrases: &'static [&'static str]) -> Command {
    Command {
        phrases,
        symbol: ",",
        placement: Placement::AfterWord,
        trigger: Trigger::NotBetweenDigits,
    }
}

const fn phrase_end(phrases: &'static [&'static str]) -> Command {
    Command {
        phrases,
        symbol: ".",
        placement: Placement::AfterWord,
        trigger: Trigger::PhraseEnd,
    }
}

const fn prefixed(
    phrases: &'static [&'static str],
    symbol: &'static str,
    placement: Placement,
) -> Command {
    Command {
        phrases,
        symbol,
        placement,
        trigger: Trigger::PrefixOnly,
    }
}

struct Language {
    code: &'static str,
    /// Forces the next command, e.g. "command comma".
    prefix: &'static str,
    /// Keeps the next command as words, e.g. "literal period".
    escape: &'static str,
    /// Articles: a command right after one is talked about, not dictated.
    articles: &'static [&'static str],
    commands: &'static [Command],
}

use Placement::{AfterWord, Break, Closing, Joining, Opening, Spaced, Suffix};

const ENGLISH: Language = Language {
    code: "en",
    prefix: "command",
    escape: "literal",
    articles: &["a", "an", "the"],
    commands: &[
        comma(&["comma"]),
        always(&["full stop"], ".", AfterWord),
        phrase_end(&["period"]),
        prefixed(&["point", "dot"], ".", AfterWord),
        always(&["semicolon"], ";", AfterWord),
        always(&["colon sign"], ":", AfterWord),
        prefixed(&["colon"], ":", AfterWord),
        always(&["question mark"], "?", AfterWord),
        always(&["exclamation mark", "exclamation point"], "!", AfterWord),
        prefixed(&["ellipsis", "ellipses"], "…", AfterWord),
        always(&["new line"], "\n", Break),
        always(&["new paragraph"], "\n\n", Break),
        always(&["open quote", "open quotes", "begin quote"], "“", Opening),
        always(
            &["close quote", "close quotes", "end quote", "unquote"],
            "”",
            Closing,
        ),
        always(&["open single quote", "begin single quote"], "‘", Opening),
        always(&["close single quote", "end single quote"], "’", Closing),
        always(
            &["open parenthesis", "open paren", "left parenthesis"],
            "(",
            Opening,
        ),
        always(
            &["close parenthesis", "close paren", "right parenthesis"],
            ")",
            Closing,
        ),
        always(&["open square bracket", "open bracket"], "[", Opening),
        always(&["close square bracket", "close bracket"], "]", Closing),
        always(
            &["open curly bracket", "open brace", "left brace"],
            "{",
            Opening,
        ),
        always(
            &["close curly bracket", "close brace", "right brace"],
            "}",
            Closing,
        ),
        always(&["em dash"], "—", Joining),
        always(&["en dash"], "–", Joining),
        prefixed(&["dash"], "–", Spaced),
        always(&["hyphen"], "-", Joining),
        always(&["forward slash"], "/", Joining),
        prefixed(&["slash"], "/", Joining),
        always(&["backslash"], "\\", Joining),
        always(&["asterisk"], "*", Suffix),
        always(&["ampersand"], "&", Spaced),
        always(&["percent sign"], "%", Suffix),
        always(&["section sign"], "§\u{A0}", Opening),
        always(&["hash sign", "number sign"], "#", Opening),
        always(&["plus sign"], "+", Spaced),
        always(&["minus sign"], "-", Spaced),
        always(&["equals sign", "equal sign"], "=", Spaced),
    ],
};

const GERMAN: Language = Language {
    code: "de",
    prefix: "Befehl",
    escape: "wörtlich",
    articles: &["ein", "eine", "einen", "einem", "einer", "eines"],
    commands: &[
        comma(&["Komma", "Beistrich"]),
        phrase_end(&["Punkt"]),
        always(&["Satzendpunkt"], ".", AfterWord),
        always(&["Strichpunkt", "Semikolon"], ";", AfterWord),
        always(&["Doppelpunkt"], ":", AfterWord),
        always(&["Fragezeichen"], "?", AfterWord),
        always(
            &[
                "Ausrufezeichen",
                "Ausrufungszeichen",
                "Ausrufzeichen",
                "Rufzeichen",
            ],
            "!",
            AfterWord,
        ),
        always(&["neue Zeile"], "\n", Break),
        always(&["neuer Absatz"], "\n\n", Break),
        // "Ausführungszeichen" is how a customer's Whisper heard "Anführungszeichen".
        always(
            &[
                "Anführungszeichen auf",
                "Anführungszeichen unten",
                "Ausführungszeichen auf",
                "Ausführungszeichen unten",
                "Gänsefüßchen unten",
            ],
            "„",
            Opening,
        ),
        always(
            &[
                "Anführungszeichen zu",
                "Anführungszeichen oben",
                "Ausführungszeichen zu",
                "Ausführungszeichen oben",
                "Gänsefüßchen oben",
            ],
            "“",
            Closing,
        ),
        always(&["Klammer auf"], "(", Opening),
        always(&["Klammer zu"], ")", Closing),
        always(&["eckige Klammer auf"], "[", Opening),
        always(&["eckige Klammer zu"], "]", Closing),
        always(&["geschweifte Klammer auf"], "{", Opening),
        always(&["geschweifte Klammer zu"], "}", Closing),
        always(&["Gedankenstrich"], "–", Spaced),
        always(&["Bindestrich"], "-", Joining),
        always(
            &["Paragraphzeichen", "Paragrafzeichen", "Paragraphenzeichen"],
            "§\u{A0}",
            Opening,
        ),
        always(&["Prozentzeichen"], "\u{A0}%", Suffix),
        always(&["Und-Zeichen", "Undzeichen", "Ampersand"], "&", Spaced),
        always(&["Rautezeichen"], "#", Opening),
        prefixed(&["Raute"], "#", Opening),
        always(&["Sternchen"], "*", Suffix),
        always(&["Pluszeichen"], "+", Spaced),
        always(&["Minuszeichen"], "-", Spaced),
        always(&["Gleichheitszeichen", "Istgleichzeichen"], "=", Spaced),
        always(&["Schrägstrich"], "/", Joining),
        always(&["umgekehrter Schrägstrich", "Backslash"], "\\", Joining),
    ],
};

const FRENCH: Language = Language {
    code: "fr",
    prefix: "commande",
    escape: "littéralement",
    articles: &["un", "une", "le", "la", "les", "des", "du"],
    commands: &[
        comma(&["virgule"]),
        phrase_end(&["point"]),
        prefixed(&["point à la ligne"], ".\n", Break),
        always(&["point-virgule", "point virgule"], "\u{202F};", AfterWord),
        always(&["deux-points"], "\u{A0}:", AfterWord),
        prefixed(&["deux points"], "\u{A0}:", AfterWord),
        always(
            &["point d'interrogation", "point d’interrogation"],
            "\u{202F}?",
            AfterWord,
        ),
        always(
            &["point d'exclamation", "point d’exclamation"],
            "\u{202F}!",
            AfterWord,
        ),
        always(&["points de suspension"], "…", AfterWord),
        always(&["nouvelle ligne", "retour à la ligne"], "\n", Break),
        prefixed(&["à la ligne"], "\n", Break),
        always(&["nouveau paragraphe"], "\n\n", Break),
        always(
            &[
                "ouvrez les guillemets",
                "ouvrir les guillemets",
                "ouvrez guillemets",
                "ouvrir guillemets",
                "ouvrir guillemet",
                "guillemet ouvrant",
                "guillemets ouvrants",
            ],
            "«\u{A0}",
            Opening,
        ),
        always(
            &[
                "fermez les guillemets",
                "fermer les guillemets",
                "fermez guillemets",
                "fermer guillemets",
                "fermer guillemet",
                "guillemet fermant",
                "guillemets fermants",
            ],
            "\u{A0}»",
            Closing,
        ),
        always(
            &[
                "ouvrez la parenthèse",
                "ouvrir la parenthèse",
                "ouvrez parenthèse",
                "ouvrir parenthèse",
                "ouvrir les parenthèses",
                "parenthèse ouvrante",
            ],
            "(",
            Opening,
        ),
        always(
            &[
                "fermez la parenthèse",
                "fermer la parenthèse",
                "fermez parenthèse",
                "fermer parenthèse",
                "fermer les parenthèses",
                "parenthèse fermante",
            ],
            ")",
            Closing,
        ),
        always(
            &[
                "ouvrez le crochet",
                "ouvrir le crochet",
                "ouvrir crochet",
                "ouvrir les crochets",
                "crochet ouvrant",
            ],
            "[",
            Opening,
        ),
        always(
            &[
                "fermez le crochet",
                "fermer le crochet",
                "fermer crochet",
                "fermer les crochets",
                "crochet fermant",
            ],
            "]",
            Closing,
        ),
        prefixed(&["tiret"], "–", Spaced),
        always(&["trait d'union", "trait d’union"], "-", Joining),
        always(
            &["barre oblique inversée", "barre oblique inverse"],
            "\\",
            Joining,
        ),
        prefixed(&["antislash"], "\\", Joining),
        always(&["barre oblique"], "/", Joining),
        prefixed(&["astérisque"], "*", Suffix),
        prefixed(&["esperluette", "perluète", "et commercial"], "&", Spaced),
        always(
            &[
                "signe pourcent",
                "signe pour cent",
                "signe pourcentage",
                "symbole pourcent",
                "symbole pour cent",
                "symbole pourcentage",
            ],
            "\u{A0}%",
            Suffix,
        ),
        always(
            &["signe paragraphe", "symbole paragraphe"],
            "§\u{A0}",
            Opening,
        ),
        always(&["signe dièse", "symbole dièse"], "#", Opening),
        always(&["signe plus", "symbole plus"], "+", Spaced),
        always(&["signe moins"], "-", Spaced),
        always(&["signe égal", "symbole égal"], "=", Spaced),
    ],
};

const SPANISH: Language = Language {
    code: "es",
    prefix: "comando",
    escape: "literal",
    articles: &["un", "una", "unos", "unas", "el", "la", "los", "las"],
    commands: &[
        comma(&["coma"]),
        phrase_end(&["punto", "punto final"]),
        always(&["punto y seguido"], ".", AfterWord),
        always(&["punto y aparte"], ".\n\n", Break),
        always(&["punto y coma"], ";", AfterWord),
        prefixed(&["dos puntos"], ":", AfterWord),
        always(
            &[
                "signo de interrogación",
                "cerrar interrogación",
                "cierra interrogación",
            ],
            "?",
            AfterWord,
        ),
        always(
            &[
                "abrir interrogación",
                "abre interrogación",
                "signo de apertura de interrogación",
            ],
            "¿",
            Opening,
        ),
        always(
            &[
                "signo de exclamación",
                "cerrar exclamación",
                "cierra exclamación",
            ],
            "!",
            AfterWord,
        ),
        always(
            &[
                "abrir exclamación",
                "abre exclamación",
                "signo de apertura de exclamación",
            ],
            "¡",
            Opening,
        ),
        always(&["puntos suspensivos"], "…", AfterWord),
        always(
            &["nueva línea", "nueva linea", "salto de línea"],
            "\n",
            Break,
        ),
        always(&["nuevo párrafo", "nuevo parrafo"], "\n\n", Break),
        always(
            &["abrir comillas", "abre comillas", "comillas izquierdas"],
            "«",
            Opening,
        ),
        always(
            &["cerrar comillas", "cierra comillas", "comillas derechas"],
            "»",
            Closing,
        ),
        always(
            &[
                "abrir paréntesis",
                "abre paréntesis",
                "abrir parentesis",
                "paréntesis de apertura",
            ],
            "(",
            Opening,
        ),
        always(
            &[
                "cerrar paréntesis",
                "cierra paréntesis",
                "cerrar parentesis",
                "paréntesis de cierre",
            ],
            ")",
            Closing,
        ),
        always(
            &["abrir corchete", "abre corchete", "corchete de apertura"],
            "[",
            Opening,
        ),
        always(
            &["cerrar corchete", "cierra corchete", "corchete de cierre"],
            "]",
            Closing,
        ),
        always(&["guion corto", "guión corto"], "-", Joining),
        prefixed(&["guion", "guión"], "-", Joining),
        always(&["barra invertida", "barra inversa"], "\\", Joining),
        always(&["barra inclinada", "barra diagonal"], "/", Joining),
        prefixed(&["barra"], "/", Joining),
        prefixed(&["asterisco"], "*", Suffix),
        always(&["y comercial"], "&", Spaced),
        prefixed(&["ampersand"], "&", Spaced),
        always(
            &["signo de porcentaje", "signo de por ciento"],
            "\u{A0}%",
            Suffix,
        ),
        always(&["signo más"], "+", Spaced),
        always(&["signo menos"], "-", Spaced),
        always(&["signo igual"], "=", Spaced),
    ],
};

const ITALIAN: Language = Language {
    code: "it",
    prefix: "comando",
    escape: "letterale",
    articles: &["un", "uno", "una", "il", "lo", "la", "i", "gli", "le"],
    commands: &[
        comma(&["virgola"]),
        phrase_end(&["punto"]),
        always(&["punto e a capo"], ".\n", Break),
        always(&["punto e virgola"], ";", AfterWord),
        prefixed(&["due punti"], ":", AfterWord),
        always(&["punto interrogativo", "punto di domanda"], "?", AfterWord),
        always(&["punto esclamativo"], "!", AfterWord),
        always(&["puntini di sospensione"], "…", AfterWord),
        always(&["nuova riga"], "\n", Break),
        prefixed(&["a capo"], "\n", Break),
        always(&["nuovo paragrafo"], "\n\n", Break),
        always(
            &["apri virgolette", "aperte virgolette", "virgolette aperte"],
            "“",
            Opening,
        ),
        always(
            &[
                "chiudi virgolette",
                "chiuse virgolette",
                "virgolette chiuse",
            ],
            "”",
            Closing,
        ),
        always(&["apri virgolette angolari"], "«", Opening),
        always(&["chiudi virgolette angolari"], "»", Closing),
        always(
            &[
                "apri parentesi",
                "aperta parentesi",
                "parentesi aperta",
                "parentesi tonda aperta",
            ],
            "(",
            Opening,
        ),
        always(
            &[
                "chiudi parentesi",
                "chiusa parentesi",
                "parentesi chiusa",
                "parentesi tonda chiusa",
            ],
            ")",
            Closing,
        ),
        always(
            &["apri parentesi quadra", "parentesi quadra aperta"],
            "[",
            Opening,
        ),
        always(
            &["chiudi parentesi quadra", "parentesi quadra chiusa"],
            "]",
            Closing,
        ),
        prefixed(&["trattino"], "-", Joining),
        always(
            &["barra rovesciata", "barra retroversa", "sbarretta inversa"],
            "\\",
            Joining,
        ),
        always(&["barra obliqua"], "/", Joining),
        prefixed(&["barra", "sbarretta"], "/", Joining),
        prefixed(&["asterisco"], "*", Suffix),
        always(&["e commerciale"], "&", Spaced),
        always(
            &["simbolo di percentuale", "segno di percentuale"],
            "%",
            Suffix,
        ),
        prefixed(&["cancelletto"], "#", Opening),
        always(&["segno più"], "+", Spaced),
        always(&["segno meno"], "-", Spaced),
        always(&["segno uguale"], "=", Spaced),
    ],
};

const PORTUGUESE: Language = Language {
    code: "pt",
    prefix: "comando",
    escape: "literal",
    articles: &["um", "uma", "uns", "umas", "o", "a", "os", "as"],
    commands: &[
        comma(&["vírgula", "virgula"]),
        phrase_end(&["ponto", "ponto final"]),
        always(&["ponto e vírgula", "ponto e virgula"], ";", AfterWord),
        prefixed(&["dois pontos"], ":", AfterWord),
        always(
            &["ponto de interrogação", "ponto de interrogacao"],
            "?",
            AfterWord,
        ),
        always(
            &["ponto de exclamação", "ponto de exclamacao"],
            "!",
            AfterWord,
        ),
        always(&["reticências"], "…", AfterWord),
        always(&["nova linha", "mudar de linha"], "\n", Break),
        always(&["novo parágrafo", "novo paragrafo"], "\n\n", Break),
        always(&["abre aspas", "abrir aspas"], "“", Opening),
        always(&["fecha aspas", "fechar aspas"], "”", Closing),
        always(
            &[
                "abre parênteses",
                "abrir parênteses",
                "abre parêntese",
                "abrir parêntese",
                "parêntese de abertura",
                "parêntese esquerdo",
            ],
            "(",
            Opening,
        ),
        always(
            &[
                "fecha parênteses",
                "fechar parênteses",
                "fecha parêntese",
                "fechar parêntese",
                "parêntese direito",
            ],
            ")",
            Closing,
        ),
        always(
            &[
                "abre colchete",
                "abre colchetes",
                "abrir colchetes",
                "abrir colchete",
                "colchete de abertura",
                "colchete esquerdo",
            ],
            "[",
            Opening,
        ),
        always(
            &[
                "fecha colchete",
                "fecha colchetes",
                "fechar colchetes",
                "fechar colchete",
                "colchete de fechamento",
                "colchete direito",
            ],
            "]",
            Closing,
        ),
        always(&["travessão"], "—", Spaced),
        always(&["hífen"], "-", Joining),
        always(&["barra invertida", "contrabarra"], "\\", Joining),
        prefixed(&["barra"], "/", Joining),
        prefixed(&["asterisco"], "*", Suffix),
        always(&["e comercial", "sinal e"], "&", Spaced),
        always(
            &[
                "sinal de porcentagem",
                "sinal de percentagem",
                "símbolo de porcentagem",
            ],
            "%",
            Suffix,
        ),
        prefixed(&["cerquilha"], "#", Opening),
        always(&["sinal de mais", "sinal de adição"], "+", Spaced),
        always(&["sinal de menos"], "-", Spaced),
        always(&["sinal de igual"], "=", Spaced),
    ],
};

const DUTCH: Language = Language {
    code: "nl",
    prefix: "commando",
    escape: "letterlijk",
    articles: &["een", "de"],
    commands: &[
        comma(&["komma"]),
        phrase_end(&["punt"]),
        always(&["puntkomma"], ";", AfterWord),
        always(
            &["dubbele punt", "dubbelepunt", "dubbelpunt"],
            ":",
            AfterWord,
        ),
        always(&["vraagteken"], "?", AfterWord),
        always(&["uitroepteken"], "!", AfterWord),
        always(&["nieuwe regel", "nieuwe lijn"], "\n", Break),
        always(&["nieuwe alinea", "nieuwe paragraaf"], "\n\n", Break),
        always(
            &[
                "aanhalingsteken openen",
                "aanhalingstekens openen",
                "aanhalingsteken open",
                "open aanhalingsteken",
            ],
            "“",
            Opening,
        ),
        always(
            &[
                "aanhalingsteken sluiten",
                "aanhalingstekens sluiten",
                "aanhalingsteken sluit",
                "sluit aanhalingsteken",
            ],
            "”",
            Closing,
        ),
        always(
            &["haakje openen", "haakje open", "open haakje"],
            "(",
            Opening,
        ),
        always(
            &["haakje sluiten", "haakje sluit", "sluit haakje"],
            ")",
            Closing,
        ),
        always(
            &[
                "vierkante haak openen",
                "vierkante haak open",
                "blokhaak open",
            ],
            "[",
            Opening,
        ),
        always(
            &[
                "vierkante haak sluiten",
                "vierkante haak sluit",
                "blokhaak sluit",
            ],
            "]",
            Closing,
        ),
        always(&["gedachtestreepje"], "–", Spaced),
        always(&["koppelteken"], "-", Joining),
        always(&["schuine streep"], "/", Joining),
        always(&["backslash"], "\\", Joining),
        always(&["ampersand", "en-teken"], "&", Spaced),
        always(&["procentteken"], "%", Suffix),
        always(&["plusteken"], "+", Spaced),
        always(&["minteken"], "-", Spaced),
        always(&["isgelijkteken", "gelijkteken"], "=", Spaced),
    ],
};

const POLISH: Language = Language {
    code: "pl",
    prefix: "polecenie",
    escape: "dosłownie",
    articles: &[],
    commands: &[
        comma(&["przecinek"]),
        phrase_end(&["kropka"]),
        always(&["średnik"], ";", AfterWord),
        always(&["dwukropek"], ":", AfterWord),
        always(&["znak zapytania"], "?", AfterWord),
        always(&["wykrzyknik"], "!", AfterWord),
        always(&["wielokropek"], "…", AfterWord),
        always(&["nowa linia", "nowy wiersz"], "\n", Break),
        always(&["nowy akapit"], "\n\n", Break),
        always(&["otwórz cudzysłów"], "„", Opening),
        always(&["zamknij cudzysłów"], "”", Closing),
        always(&["otwórz nawias kwadratowy"], "[", Opening),
        always(&["zamknij nawias kwadratowy"], "]", Closing),
        always(&["otwórz nawias"], "(", Opening),
        always(&["zamknij nawias"], ")", Closing),
        prefixed(&["myślnik"], "–", Spaced),
        prefixed(&["łącznik", "kreska"], "-", Joining),
        always(&["ukośnik odwrotny", "ukośnik wsteczny"], "\\", Joining),
        prefixed(&["ukośnik"], "/", Joining),
        prefixed(&["gwiazdka"], "*", Suffix),
        always(&["ampersand"], "&", Spaced),
        always(&["znak procentu"], "\u{A0}%", Suffix),
        prefixed(&["procent"], "\u{A0}%", Suffix),
        always(&["znak plus"], "+", Spaced),
        always(&["znak minus"], "-", Spaced),
        always(&["znak równości"], "=", Spaced),
    ],
};

const LANGUAGES: &[Language] = &[
    ENGLISH, GERMAN, FRENCH, SPANISH, ITALIAN, PORTUGUESE, DUTCH, POLISH,
];

/// Marks a recognizer adds on its own around a spoken command word.
const ENGINE_MARKS: &[char] = &[',', '.', ';', ':', '!', '?'];
/// Marks that cannot sensibly precede a mark or break the user dictated.
const CLAUSE_MARKS: &[char] = &[',', ';', ':'];
const CLAUSE_AND_PERIOD_MARKS: &[char] = &[',', ';', ':', '.'];
/// Characters that attach to the preceding word, so no space goes before them.
const ATTACHES_LEFT: &[char] = &[
    ',', '.', ';', ':', '!', '?', ')', ']', '}', '“', '”', '"', '\'', '’', '…', '»', '\u{A0}',
    '\u{202F}',
];
/// After these, a following symbol needs no space of its own.
const OPENING_SYMBOLS: &[char] = &[
    '(', '[', '{', '„', '«', '‘', '¿', '¡', '#', '\u{A0}', '\u{202F}',
];
const SPACES: [char; 2] = [' ', '\t'];

struct Matcher {
    language: &'static Language,
    regex: Regex,
    commands: HashMap<String, &'static Command>,
}

fn normalize_phrase(phrase: &str) -> String {
    phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn build_matcher(language: &'static Language) -> Matcher {
    let mut phrases: Vec<(&'static str, &'static Command)> = language
        .commands
        .iter()
        .flat_map(|command| command.phrases.iter().map(move |phrase| (*phrase, command)))
        .collect();
    // Longest first, so "eckige Klammer auf" wins over "Klammer auf".
    phrases.sort_by_key(|(phrase, _)| std::cmp::Reverse(phrase.chars().count()));
    let pattern = phrases
        .iter()
        .map(|(phrase, _)| {
            phrase
                .split_whitespace()
                .map(regex::escape)
                .collect::<Vec<_>>()
                .join(r"\s+")
        })
        .collect::<Vec<_>>()
        .join("|");
    let regex = RegexBuilder::new(&format!("(?:{pattern})"))
        .case_insensitive(true)
        .build()
        .expect("spoken punctuation pattern is valid");
    let commands = phrases
        .into_iter()
        .map(|(phrase, command)| (normalize_phrase(phrase), command))
        .collect();
    Matcher {
        language,
        regex,
        commands,
    }
}

static MATCHERS: LazyLock<Vec<Matcher>> =
    LazyLock::new(|| LANGUAGES.iter().map(build_matcher).collect());

fn matcher_for(transcript_language: Option<&str>) -> Option<&'static Matcher> {
    let base = transcript_language?.split(['-', '_']).next()?.trim();
    MATCHERS
        .iter()
        .find(|matcher| matcher.language.code.eq_ignore_ascii_case(base))
}

impl Matcher {
    fn command(&self, matched: &str) -> Option<&'static Command> {
        self.commands.get(&normalize_phrase(matched)).copied()
    }

    /// The command starting exactly at `pos`, if it is a whole word.
    fn command_at(&self, text: &str, pos: usize) -> Option<&'static Command> {
        let found = self.regex.find_at(text, pos)?;
        (found.start() == pos && is_whole_word(text, found.start(), found.end()))
            .then(|| self.command(found.as_str()))
            .flatten()
    }
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '\'' || ch == '’'
}

/// A command must be a whole word: "Strichpunkte" or "Doppelpunkt-Regel" stay literal.
fn is_whole_word(text: &str, start: usize, end: usize) -> bool {
    let left_ok = text[..start]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_word_char(ch));
    let right_ok = text[end..]
        .chars()
        .next()
        .is_none_or(|ch| !is_word_char(ch));
    left_ok && right_ok
}

fn skip_spaces(text: &str, pos: usize) -> usize {
    text.len() - text[pos..].trim_start_matches(SPACES).len()
}

/// Skip spaces, then one recognizer-added mark from `marks`, then spaces.
fn skip_following_mark(text: &str, pos: usize, marks: &[char]) -> usize {
    let pos = skip_spaces(text, pos);
    match text[pos..].chars().next() {
        Some(ch) if marks.contains(&ch) => skip_spaces(text, pos + ch.len_utf8()),
        _ => pos,
    }
}

fn trim_trailing_spaces(out: &mut String) {
    let kept = out.trim_end_matches(SPACES).len();
    out.truncate(kept);
}

/// Remove spaces and one recognizer-added mark from `marks` before a symbol.
fn drop_preceding_mark(out: &mut String, marks: &[char]) {
    trim_trailing_spaces(out);
    if out
        .chars()
        .next_back()
        .is_some_and(|ch| marks.contains(&ch))
    {
        out.pop();
        trim_trailing_spaces(out);
    }
}

fn push_space_before_symbol(out: &mut String) {
    if out
        .chars()
        .next_back()
        .is_some_and(|ch| ch != '\n' && !OPENING_SYMBOLS.contains(&ch))
    {
        out.push(' ');
    }
}

fn push_space_before_next_word(out: &mut String, text: &str, pos: usize) {
    if text[pos..]
        .chars()
        .next()
        .is_some_and(|ch| ch != '\n' && !ATTACHES_LEFT.contains(&ch))
    {
        out.push(' ');
    }
}

fn ends_sentence(symbol: &str) -> bool {
    symbol
        .trim_start_matches(['\u{A0}', '\u{202F}'])
        .starts_with(['.', '?', '!'])
}

/// Write `command` to `out` and return where copying the transcript resumes.
fn render(out: &mut String, text: &str, after: usize, command: &Command) -> usize {
    let preceding = if ends_sentence(command.symbol) {
        CLAUSE_AND_PERIOD_MARKS
    } else {
        CLAUSE_MARKS
    };
    match command.placement {
        Placement::AfterWord => {
            drop_preceding_mark(out, preceding);
            out.push_str(command.symbol);
            let pos = skip_following_mark(text, after, ENGINE_MARKS);
            push_space_before_next_word(out, text, pos);
            pos
        }
        Placement::Suffix => {
            trim_trailing_spaces(out);
            out.push_str(command.symbol);
            let pos = skip_spaces(text, after);
            push_space_before_next_word(out, text, pos);
            pos
        }
        Placement::Break => {
            drop_preceding_mark(out, preceding);
            out.push_str(command.symbol);
            skip_following_mark(text, after, CLAUSE_AND_PERIOD_MARKS)
        }
        Placement::Opening => {
            trim_trailing_spaces(out);
            push_space_before_symbol(out);
            out.push_str(command.symbol);
            skip_following_mark(text, after, CLAUSE_AND_PERIOD_MARKS)
        }
        Placement::Closing => {
            drop_preceding_mark(out, CLAUSE_MARKS);
            out.push_str(command.symbol);
            let pos = skip_spaces(text, after);
            push_space_before_next_word(out, text, pos);
            pos
        }
        Placement::Spaced => {
            drop_preceding_mark(out, &[',']);
            push_space_before_symbol(out);
            out.push_str(command.symbol);
            let pos = skip_following_mark(text, after, &[',']);
            push_space_before_next_word(out, text, pos);
            pos
        }
        Placement::Joining => {
            trim_trailing_spaces(out);
            out.push_str(command.symbol);
            skip_spaces(text, after)
        }
    }
}

/// What the word directly before a command says about it.
enum Preceding {
    /// The prefix word starting at this byte offset: convert.
    Prefix(usize),
    /// The escape word starting at this byte offset: keep the command as words.
    Escape(usize),
    /// An article: the command is talked about, not dictated.
    Article,
    Other,
}

/// Classify the word right before a command. One recognizer-added mark
/// between a prefix or escape and the command is tolerated ("command, comma").
fn preceding_word(before: &str, language: &Language) -> Preceding {
    let adjacent = before.trim_end_matches(SPACES);
    let after_mark = adjacent
        .strip_suffix(ENGINE_MARKS)
        .map(|rest| rest.trim_end_matches(SPACES));
    let last_word = |text: &str| {
        text.char_indices()
            .rev()
            .take_while(|(_, ch)| is_word_char(*ch))
            .last()
            .map(|(start, _)| (start, text[start..].to_lowercase()))
    };
    let is_word = |word: &str, candidate: &str| word == candidate.to_lowercase();
    for text in [Some(adjacent), after_mark].into_iter().flatten() {
        if let Some((start, word)) = last_word(text) {
            if is_word(&word, language.prefix) {
                return Preceding::Prefix(start);
            }
            if is_word(&word, language.escape) {
                return Preceding::Escape(start);
            }
        }
    }
    match last_word(adjacent) {
        Some((_, word))
            if language
                .articles
                .iter()
                .any(|article| is_word(&word, article)) =>
        {
            Preceding::Article
        }
        _ => Preceding::Other,
    }
}

/// End of the dictation, a recognizer's sentence end or line break, or an
/// unambiguous command. A speculative period word never qualifies another.
fn at_phrase_end(matcher: &Matcher, text: &str, end: usize) -> bool {
    let pos = skip_spaces(text, end);
    match text[pos..].chars().next() {
        None | Some('.' | '?' | '!' | '…' | '\n') => true,
        Some(_) => {
            let pos = skip_following_mark(text, pos, &[',']);
            matcher.command_at(text, pos).is_some_and(|next| {
                matches!(next.trigger, Trigger::Always | Trigger::NotBetweenDigits)
            })
        }
    }
}

fn between_digits(text: &str, start: usize, end: usize) -> bool {
    let previous = text[..start].trim_end_matches(SPACES).chars().next_back();
    let next = text[skip_spaces(text, end)..].chars().next();
    previous.is_some_and(|ch| ch.is_ascii_digit()) && next.is_some_and(|ch| ch.is_ascii_digit())
}

fn triggers(command: &Command, matcher: &Matcher, text: &str, start: usize, end: usize) -> bool {
    match command.trigger {
        Trigger::Always => true,
        Trigger::NotBetweenDigits => !between_digits(text, start, end),
        Trigger::PhraseEnd => at_phrase_end(matcher, text, end),
        Trigger::PrefixOnly => false,
    }
}

/// Replace spoken punctuation commands with their symbols for the transcript
/// language (English, German, French, Spanish, Italian, Portuguese, Dutch,
/// Polish; regional codes such as `de-AT` use their base table).
///
/// Returns `None` for other or unknown languages, or when nothing changed.
pub(crate) fn apply_spoken_punctuation(
    text: &str,
    transcript_language: Option<&str>,
) -> Option<String> {
    let matcher = matcher_for(transcript_language)?;
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    let mut changed = false;
    for found in matcher.regex.find_iter(text) {
        if found.start() < cursor || !is_whole_word(text, found.start(), found.end()) {
            continue;
        }
        let Some(command) = matcher.command(found.as_str()) else {
            continue;
        };
        let before = &text[cursor..found.start()];
        match preceding_word(before, matcher.language) {
            Preceding::Escape(start) => {
                out.push_str(&before[..start]);
                out.push_str(found.as_str());
                cursor = found.end();
                changed = true;
                continue;
            }
            Preceding::Prefix(start) => out.push_str(&before[..start]),
            Preceding::Article => continue,
            Preceding::Other => {
                if !triggers(command, matcher, text, found.start(), found.end()) {
                    continue;
                }
                out.push_str(before);
            }
        }
        cursor = render(&mut out, text, found.end(), command);
        changed = true;
    }
    if !changed {
        return None;
    }
    out.push_str(&text[cursor..]);
    Some(out)
}

/// One command as shown in Settings.
#[derive(Debug, Clone, Serialize)]
pub struct SpokenPunctuationCommand {
    pub phrases: Vec<&'static str>,
    pub symbol: &'static str,
    pub trigger: SpokenPunctuationTrigger,
}

/// One language's commands as shown in Settings.
#[derive(Debug, Clone, Serialize)]
pub struct SpokenPunctuationLanguage {
    pub code: &'static str,
    pub prefix: &'static str,
    pub escape: &'static str,
    pub commands: Vec<SpokenPunctuationCommand>,
}

/// The command tables, for the Settings reference.
pub fn spoken_punctuation_reference() -> Vec<SpokenPunctuationLanguage> {
    LANGUAGES
        .iter()
        .map(|language| SpokenPunctuationLanguage {
            code: language.code,
            prefix: language.prefix,
            escape: language.escape,
            commands: language
                .commands
                .iter()
                .map(|command| SpokenPunctuationCommand {
                    phrases: command.phrases.to_vec(),
                    symbol: command.symbol.trim_matches(['\u{A0}', '\u{202F}']),
                    trigger: command.trigger,
                })
                .collect(),
        })
        .collect()
}
