// Whisper supported languages
// Based on OpenAI Whisper's tokenizer.py and official documentation
// These are all the languages that Whisper models can handle

use once_cell::sync::Lazy;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}

// All supported Whisper languages
pub static SUPPORTED_LANGUAGES: Lazy<HashMap<&'static str, Language>> = Lazy::new(|| {
    let languages = vec![
        Language {
            code: "en",
            name: "English",
        },
        Language {
            code: "zh",
            name: "Chinese",
        },
        Language {
            code: "de",
            name: "German",
        },
        Language {
            code: "es",
            name: "Spanish",
        },
        Language {
            code: "ru",
            name: "Russian",
        },
        Language {
            code: "ko",
            name: "Korean",
        },
        Language {
            code: "fr",
            name: "French",
        },
        Language {
            code: "ja",
            name: "Japanese",
        },
        Language {
            code: "pt",
            name: "Portuguese",
        },
        Language {
            code: "tr",
            name: "Turkish",
        },
        Language {
            code: "pl",
            name: "Polish",
        },
        Language {
            code: "ca",
            name: "Catalan",
        },
        Language {
            code: "nl",
            name: "Dutch",
        },
        Language {
            code: "ar",
            name: "Arabic",
        },
        Language {
            code: "sv",
            name: "Swedish",
        },
        Language {
            code: "it",
            name: "Italian",
        },
        Language {
            code: "id",
            name: "Indonesian",
        },
        Language {
            code: "hi",
            name: "Hindi",
        },
        Language {
            code: "fi",
            name: "Finnish",
        },
        Language {
            code: "vi",
            name: "Vietnamese",
        },
        Language {
            code: "he",
            name: "Hebrew",
        },
        Language {
            code: "uk",
            name: "Ukrainian",
        },
        Language {
            code: "el",
            name: "Greek",
        },
        Language {
            code: "ms",
            name: "Malay",
        },
        Language {
            code: "cs",
            name: "Czech",
        },
        Language {
            code: "ro",
            name: "Romanian",
        },
        Language {
            code: "da",
            name: "Danish",
        },
        Language {
            code: "hu",
            name: "Hungarian",
        },
        Language {
            code: "ta",
            name: "Tamil",
        },
        Language {
            code: "no",
            name: "Norwegian",
        },
        Language {
            code: "th",
            name: "Thai",
        },
        Language {
            code: "ur",
            name: "Urdu",
        },
        Language {
            code: "hr",
            name: "Croatian",
        },
        Language {
            code: "bg",
            name: "Bulgarian",
        },
        Language {
            code: "lt",
            name: "Lithuanian",
        },
        Language {
            code: "la",
            name: "Latin",
        },
        Language {
            code: "mi",
            name: "Maori",
        },
        Language {
            code: "ml",
            name: "Malayalam",
        },
        Language {
            code: "cy",
            name: "Welsh",
        },
        Language {
            code: "sk",
            name: "Slovak",
        },
        Language {
            code: "te",
            name: "Telugu",
        },
        Language {
            code: "fa",
            name: "Persian",
        },
        Language {
            code: "lv",
            name: "Latvian",
        },
        Language {
            code: "bn",
            name: "Bengali",
        },
        Language {
            code: "sr",
            name: "Serbian",
        },
        Language {
            code: "az",
            name: "Azerbaijani",
        },
        Language {
            code: "sl",
            name: "Slovenian",
        },
        Language {
            code: "kn",
            name: "Kannada",
        },
        Language {
            code: "et",
            name: "Estonian",
        },
        Language {
            code: "mk",
            name: "Macedonian",
        },
        Language {
            code: "br",
            name: "Breton",
        },
        Language {
            code: "eu",
            name: "Basque",
        },
        Language {
            code: "is",
            name: "Icelandic",
        },
        Language {
            code: "hy",
            name: "Armenian",
        },
        Language {
            code: "ne",
            name: "Nepali",
        },
        Language {
            code: "mn",
            name: "Mongolian",
        },
        Language {
            code: "bs",
            name: "Bosnian",
        },
        Language {
            code: "kk",
            name: "Kazakh",
        },
        Language {
            code: "sq",
            name: "Albanian",
        },
        Language {
            code: "sw",
            name: "Swahili",
        },
        Language {
            code: "gl",
            name: "Galician",
        },
        Language {
            code: "mr",
            name: "Marathi",
        },
        Language {
            code: "pa",
            name: "Punjabi",
        },
        Language {
            code: "si",
            name: "Sinhala",
        },
        Language {
            code: "km",
            name: "Khmer",
        },
        Language {
            code: "sn",
            name: "Shona",
        },
        Language {
            code: "yo",
            name: "Yoruba",
        },
        Language {
            code: "so",
            name: "Somali",
        },
        Language {
            code: "af",
            name: "Afrikaans",
        },
        Language {
            code: "oc",
            name: "Occitan",
        },
        Language {
            code: "ka",
            name: "Georgian",
        },
        Language {
            code: "be",
            name: "Belarusian",
        },
        Language {
            code: "tg",
            name: "Tajik",
        },
        Language {
            code: "sd",
            name: "Sindhi",
        },
        Language {
            code: "gu",
            name: "Gujarati",
        },
        Language {
            code: "am",
            name: "Amharic",
        },
        Language {
            code: "yi",
            name: "Yiddish",
        },
        Language {
            code: "lo",
            name: "Lao",
        },
        Language {
            code: "uz",
            name: "Uzbek",
        },
        Language {
            code: "fo",
            name: "Faroese",
        },
        Language {
            code: "ht",
            name: "Haitian Creole",
        },
        Language {
            code: "ps",
            name: "Pashto",
        },
        Language {
            code: "tk",
            name: "Turkmen",
        },
        Language {
            code: "nn",
            name: "Nynorsk",
        },
        Language {
            code: "mt",
            name: "Maltese",
        },
        Language {
            code: "sa",
            name: "Sanskrit",
        },
        Language {
            code: "lb",
            name: "Luxembourgish",
        },
        Language {
            code: "my",
            name: "Myanmar",
        },
        Language {
            code: "bo",
            name: "Tibetan",
        },
        Language {
            code: "tl",
            name: "Tagalog",
        },
        Language {
            code: "mg",
            name: "Malagasy",
        },
        Language {
            code: "as",
            name: "Assamese",
        },
        Language {
            code: "tt",
            name: "Tatar",
        },
        Language {
            code: "haw",
            name: "Hawaiian",
        },
        Language {
            code: "ln",
            name: "Lingala",
        },
        Language {
            code: "ha",
            name: "Hausa",
        },
        Language {
            code: "ba",
            name: "Bashkir",
        },
        Language {
            code: "jw",
            name: "Javanese",
        },
        Language {
            code: "su",
            name: "Sundanese",
        },
        Language {
            code: "yue",
            name: "Cantonese",
        },
    ];

    languages
        .into_iter()
        .map(|lang| (lang.code, lang))
        .collect()
});

/// Check if a language code is supported by Whisper
pub fn is_language_supported(code: &str) -> bool {
    SUPPORTED_LANGUAGES.contains_key(code)
}

/// Get the language name for a given code
#[cfg(test)]
pub fn get_language_name(code: &str) -> Option<&'static str> {
    SUPPORTED_LANGUAGES.get(code).map(|lang| lang.name)
}

/// Validate and normalize a language code
/// Returns the validated code or "en" as default
pub fn validate_language(code: Option<&str>) -> &'static str {
    match code {
        Some(lang_code) => {
            if is_language_supported(lang_code) {
                // Find the static str in our map to return a 'static reference
                SUPPORTED_LANGUAGES
                    .get(lang_code)
                    .map(|lang| lang.code)
                    .unwrap_or("en")
            } else {
                log::warn!(
                    "Invalid language code '{}', defaulting to English",
                    lang_code
                );
                "en"
            }
        }
        None => "en", // Default to English
    }
}

/// Recognition accepts Auto, while explicit output-language validation above
/// intentionally does not. Share native parameter mapping with the GPU sidecar.
pub(crate) fn whisper_language(language: Option<&str>) -> &'static str {
    let language = crate::transcription::language::whisper_language(language);
    if language.is_empty() {
        ""
    } else {
        validate_language(Some(language))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_support() {
        assert!(is_language_supported("en"));
        assert!(!is_language_supported("auto")); // Recognition mode, not an explicit language code
        assert!(is_language_supported("zh"));
        assert!(!is_language_supported("xyz"));
        assert!(!is_language_supported(""));
    }

    #[test]
    fn test_validate_language() {
        assert_eq!(validate_language(Some("en")), "en");
        assert_eq!(validate_language(Some("auto")), "en"); // Output-language validation stays explicit
        assert_eq!(validate_language(Some("invalid")), "en");
        assert_eq!(validate_language(None), "en");
    }

    #[test]
    fn test_get_language_name() {
        assert_eq!(get_language_name("en"), Some("English"));
        assert_eq!(get_language_name("zh"), Some("Chinese"));
        assert_eq!(get_language_name("invalid"), None);
    }
}
