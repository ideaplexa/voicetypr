#[cfg(test)]
mod tests {
    use crate::whisper::languages::*;

    #[test]
    fn test_language_validation() {
        // Valid languages should be accepted
        assert_eq!(validate_language(Some("en")), "en");
        assert_eq!(validate_language(Some("es")), "es");
        assert_eq!(validate_language(Some("zh")), "zh");
        // The explicit-language validator excludes recognition modes.
        assert_eq!(validate_language(Some("auto")), "en");

        // Invalid languages should default to English
        assert_eq!(validate_language(Some("xyz")), "en");
        assert_eq!(validate_language(Some("")), "en");
        assert_eq!(validate_language(None), "en");
    }

    #[test]
    fn test_language_names() {
        assert_eq!(get_language_name("en"), Some("English"));
        assert_eq!(get_language_name("es"), Some("Spanish"));
        // Auto is labelled separately by the spoken-language controls.
        assert_eq!(get_language_name("auto"), None);
        assert_eq!(get_language_name("invalid"), None);
    }

    #[test]
    fn test_all_frontend_languages_are_supported() {
        // List of all languages from GeneralSettings.tsx
        let frontend_languages = vec![
            "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar",
            "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu",
            "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa",
            "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn",
            "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc",
            "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn",
            "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw",
            "su", "yue",
        ];

        for lang in frontend_languages {
            assert!(
                is_language_supported(lang),
                "Language '{}' from frontend is not in backend supported list",
                lang
            );
        }
    }
}
