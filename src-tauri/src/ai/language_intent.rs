//! Translation is a user request, never a guess about a missing source language.
use crate::commands::settings::FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TranslationIntent<'a> {
    Preserve,
    TranslateTo(&'a str),
}

pub(crate) fn present_language(language: Option<&str>) -> Option<&str> {
    language.map(str::trim).filter(|language| {
        !language.is_empty()
            && !language.eq_ignore_ascii_case("auto")
            && *language != FINAL_TEXT_LANGUAGE_SAME_AS_TRANSCRIPT
            && crate::whisper::languages::is_language_supported(
                &base(language).to_ascii_lowercase(),
            )
    })
}

fn base(language: &str) -> &str {
    language.split(['-', '_']).next().unwrap_or(language)
}

impl<'a> TranslationIntent<'a> {
    pub(crate) fn resolve(source: Option<&str>, requested_target: Option<&'a str>) -> Self {
        let Some(target) = present_language(requested_target) else {
            return Self::Preserve;
        };
        if present_language(source)
            .is_some_and(|source| base(source).eq_ignore_ascii_case(base(target)))
        {
            Self::Preserve
        } else {
            Self::TranslateTo(target)
        }
    }

    pub(crate) fn target(self) -> Option<&'a str> {
        match self {
            Self::Preserve => None,
            Self::TranslateTo(target) => Some(target),
        }
    }
}
