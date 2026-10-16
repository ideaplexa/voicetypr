//! Curated, immutable GGUF artifacts. No runtime registry discovery or model
//! auto-download happens in the native process.
use once_cell::sync::Lazy;
use std::collections::HashMap;

// Keep aligned with DEFAULT_LOCAL_MODEL_NAME in src/lib/model-display.ts.
pub const DEFAULT_MODEL_ID: &str = "parakeet-ultra-q8_0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelFamily {
    ParakeetUltra,
    R2t2,
}

pub struct ModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub family: ModelFamily,
    pub filename: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

impl ModelDefinition {
    pub fn backend(&self) -> &'static str {
        match self.family {
            ModelFamily::ParakeetUltra => "parakeet",
            ModelFamily::R2t2 => "qwen3",
        }
    }
    pub fn repository(&self) -> &'static str {
        match self.family {
            ModelFamily::ParakeetUltra => "cstr/parakeet-ultra-GGUF",
            ModelFamily::R2t2 => "cstr/confucius4-r2t2-GGUF",
        }
    }
    pub fn revision(&self) -> &'static str {
        match self.family {
            ModelFamily::ParakeetUltra => "252cd632a21e98ba5edbdeb61c7274d01c54cb73",
            ModelFamily::R2t2 => "6a9aa41833f577a7a2f5d0a2ed61d8250c0c57b4",
        }
    }
    pub fn url_for(&self, filename: &str) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repository(),
            self.revision(),
            filename
        )
    }
    pub fn languages(&self) -> &'static [&'static str] {
        match self.family {
            ModelFamily::ParakeetUltra => &[
                "en", "es", "fr", "de", "bg", "hr", "cs", "da", "nl", "et", "fi", "el", "hu", "it",
                "lv", "lt", "mt", "pl", "pt", "ro", "sk", "sl", "sv", "ru", "uk",
            ],
            ModelFamily::R2t2 => &[
                "en", "zh", "yue", "ja", "ko", "de", "fr", "ru", "pt", "es", "it", "ar", "hi",
                "th", "vi", "id", "tr", "nl", "sv", "da", "fi", "pl", "cs", "tl", "fa", "el", "hu",
                "mk", "ro", "ms",
            ],
        }
    }
}

pub static MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: DEFAULT_MODEL_ID,
        display_name: "Parakeet Ultra Q8",
        family: ModelFamily::ParakeetUltra,
        filename: "parakeet-ultra-q8_0.gguf",
        size: 674_342_400,
        sha256: "ebf1186c3dc7e77f71877a5380a73e39d5c0aaf5cb55e65e56b077b1b2aacef1",
    },
    ModelDefinition {
        id: "confucius4-r2t2-q4_k",
        display_name: "R2T2 Q4_K",
        family: ModelFamily::R2t2,
        filename: "confucius4-r2t2-q4_k.gguf",
        size: 1_490_915_264,
        sha256: "a754788730d7b2a62542de295d37e431e79795d5bea418fa0e4f9618f0f8e3b3",
    },
    ModelDefinition {
        id: "confucius4-r2t2-q8_0",
        display_name: "R2T2 Q8",
        family: ModelFamily::R2t2,
        filename: "confucius4-r2t2-q8_0.gguf",
        size: 2_506_723_264,
        sha256: "6d8936c05befde4775c442ddfca61e3964495781070370b0a7dbdc10f652098f",
    },
];

static INDEX: Lazy<HashMap<&'static str, &'static ModelDefinition>> =
    Lazy::new(|| MODELS.iter().map(|model| (model.id, model)).collect());

pub fn get(id: &str) -> Option<&'static ModelDefinition> {
    INDEX.get(id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_pinned_and_has_unique_registered_ids() {
        assert_eq!(INDEX.len(), 3);
        for model in MODELS {
            assert_eq!(model.sha256.len(), 64);
            assert!(model.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert_eq!(model.revision().len(), 40);
            assert!(!model.url_for(model.filename).contains("/main/"));
            assert!(get(model.id).is_some());
        }
        assert!(get("../model").is_none());
        assert!(MODELS[1].size < MODELS[2].size);
    }
    #[test]
    fn model_languages_are_explicit_and_match_the_shared_selector() {
        assert_eq!(MODELS[0].languages().len(), 25);
        assert_eq!(MODELS[1].languages().len(), 30);
        for model in MODELS {
            for language in model.languages() {
                assert!(crate::whisper::languages::is_language_supported(language));
            }
        }
    }
}
