//! Small, dependency-free language boundary shared with the Vulkan sidecar.

/// A real hint/metadata language, or no constraint for automatic recognition.
pub fn explicit_language(language: Option<&str>) -> Option<&str> {
    language
        .map(str::trim)
        .filter(|language| !language.is_empty() && *language != "auto")
}

/// Whisper accepts an empty string for automatic detection. An absent argument
/// retains the historical default; a CLI override must remain distinct from it.
pub fn whisper_language(language: Option<&str>) -> &str {
    match language {
        Some(language) => explicit_language(Some(language)).unwrap_or(""),
        None => "en",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_language_uses_whispers_empty_string_contract() {
        assert_eq!(whisper_language(Some("auto")), "");
        assert_eq!(whisper_language(Some("")), "");
        assert_eq!(whisper_language(Some(" auto ")), "");
        assert_eq!(whisper_language(Some("fr")), "fr");
        assert_eq!(whisper_language(None), "en");
    }

    #[test]
    fn auto_language_is_not_an_explicit_hint() {
        assert_eq!(explicit_language(Some("auto")), None);
        assert_eq!(explicit_language(Some("")), None);
        assert_eq!(explicit_language(None), None);
        assert_eq!(explicit_language(Some(" fr ")), Some("fr"));
    }
}
