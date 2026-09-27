#![allow(dead_code)]

use once_cell::sync::Lazy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParakeetModelKind {
    TdtV2,
    TdtV3,
    UnifiedEnglish640,
    NemotronMultilingual1120,
}

#[derive(Debug, Clone)]
pub struct ParakeetModelFile {
    pub filename: &'static str,
}

#[derive(Debug, Clone)]
pub struct ParakeetModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub repo_id: &'static str,
    pub cache_subdir: &'static str,
    pub description: &'static str,
    pub languages: &'static [&'static str],
    pub recommended: bool,
    pub speed_score: u8,
    pub accuracy_score: u8,
    pub files: &'static [ParakeetModelFile],
    pub estimated_size: u64,
    pub kind: ParakeetModelKind,
    /// If true, this model has additional restrictions beyond the base Apple Silicon requirement.
    /// Note: ALL Parakeet models require Apple Silicon (FluidAudio uses Apple Neural Engine).
    /// This flag indicates models that have extra compatibility issues (e.g., V2 SIGFPE crashes).
    pub apple_silicon_only: bool,
}

/// Check if running on Apple Silicon (aarch64)
pub fn is_apple_silicon() -> bool {
    std::env::consts::ARCH == "aarch64"
}

/// Get available models for the current architecture.
///
/// **Important**: FluidAudio ASR requires Apple Silicon (Apple Neural Engine).
/// On Intel Macs, this returns an empty vector because FluidAudio throws
/// `ASRError.unsupportedPlatform` at runtime for ALL Parakeet models.
///
/// Intel Mac users should use Whisper models instead (CPU-only mode).
pub fn get_available_models() -> Vec<&'static ParakeetModelDefinition> {
    let arch = std::env::consts::ARCH;

    // FluidAudio ASR requires Apple Silicon - no Parakeet models work on Intel Macs
    // FluidAudio throws ASRError.unsupportedPlatform("Parakeet models require Apple Silicon")
    if !is_apple_silicon() {
        log::debug!(
            "Parakeet unavailable on Intel Mac - FluidAudio requires Apple Neural Engine (arch: {})",
            arch
        );
        return vec![];
    }

    // On Apple Silicon, return all available models
    AVAILABLE_MODELS.iter().collect()
}

// Parakeet models using Swift/FluidAudio sidecar
// These models are macOS-only and use Apple Neural Engine for acceleration
pub static AVAILABLE_MODELS: Lazy<Vec<ParakeetModelDefinition>> = Lazy::new(|| {
    vec![
        ParakeetModelDefinition {
            id: "parakeet-tdt-0.6b-v3",
            display_name: "Parakeet V3",
            repo_id: "FluidInference/parakeet-tdt-0.6b-v3-coreml",
            cache_subdir: "parakeet-tdt-0.6b-v3",
            description: "Native Swift transcription using Apple Neural Engine",
            languages: &[
                "en", "es", "fr", "de", "bg", "hr", "cs", "da", "nl", "et", "fi", "el", "hu", "it",
                "lv", "lt", "mt", "pl", "pt", "ro", "sk", "sl", "sv", "ru", "uk",
            ],
            recommended: true,
            speed_score: 9,
            accuracy_score: 9,
            files: &[
                ParakeetModelFile {
                    filename: "Preprocessor.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "Encoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "Decoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "JointDecisionv3.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "parakeet_vocab.json",
                },
            ],
            estimated_size: 500_000_000,
            kind: ParakeetModelKind::TdtV3,
            apple_silicon_only: false,
        },
        ParakeetModelDefinition {
            id: "parakeet-tdt-0.6b-v2",
            display_name: "Parakeet V2 (English)",
            repo_id: "FluidInference/parakeet-tdt-0.6b-v2-coreml",
            cache_subdir: "parakeet-tdt-0.6b-v2",
            description: "Native Swift transcription optimized for English",
            languages: &["en"],
            recommended: false,
            speed_score: 9,
            accuracy_score: 8,
            files: &[
                ParakeetModelFile {
                    filename: "Preprocessor.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "Encoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "Decoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "JointDecision.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "parakeet_vocab.json",
                },
            ],
            estimated_size: 480_000_000,
            kind: ParakeetModelKind::TdtV2,
            apple_silicon_only: true,
        },
        ParakeetModelDefinition {
            id: "parakeet-unified-640ms",
            display_name: "Parakeet Unified (English)",
            repo_id: "FluidInference/parakeet-unified-en-0.6b-coreml",
            cache_subdir: "parakeet-unified-en-0.6b",
            description: "Native English streaming with punctuation and capitalization",
            languages: &["en"],
            recommended: true,
            speed_score: 9,
            accuracy_score: 10,
            files: &[
                ParakeetModelFile {
                    filename: "parakeet_unified_encoder_streaming_70_7_1_int8.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "parakeet_unified_decoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "parakeet_unified_joint_decision_single_step.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "vocab.json",
                },
                ParakeetModelFile {
                    filename: "metadata.json",
                },
            ],
            estimated_size: 620_000_000,
            kind: ParakeetModelKind::UnifiedEnglish640,
            apple_silicon_only: false,
        },
        ParakeetModelDefinition {
            id: "nemotron-multilingual-1120ms",
            display_name: "Nemotron Multilingual",
            repo_id: "FluidInference/Nemotron-3.5-ASR-Streaming-Multilingual-0.6b-CoreML",
            cache_subdir: "nemotron-multilingual/multilingual/1120ms",
            description: "Native multilingual streaming with language detection",
            // Bare codes that both Voicetypr's selector and the pinned 1120ms
            // bundle's metadata.json prompt_dictionary can represent.
            languages: &[
                "en", "af", "am", "ar", "az", "bg", "bn", "cs", "da", "de", "el", "es", "et", "fa",
                "fi", "fr", "gu", "ha", "haw", "he", "hi", "hr", "hu", "hy", "id", "it", "ja",
                "ka", "km", "kn", "ko", "ln", "lt", "lv", "mi", "ml", "mr", "ms", "mt", "ne", "nl",
                "nn", "no", "pl", "pt", "ro", "ru", "si", "sk", "sl", "so", "sv", "sw", "ta", "te",
                "tg", "th", "tr", "uk", "ur", "uz", "vi", "yo", "zh",
            ],
            recommended: true,
            speed_score: 8,
            accuracy_score: 9,
            files: &[
                ParakeetModelFile {
                    filename: "preprocessor.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "encoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "decoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "joint.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "tokenizer.json",
                },
                ParakeetModelFile {
                    filename: "metadata.json",
                },
            ],
            estimated_size: 665_000_000,
            kind: ParakeetModelKind::NemotronMultilingual1120,
            apple_silicon_only: false,
        },
    ]
});

#[cfg(test)]
mod tests {
    use super::{ParakeetModelKind, AVAILABLE_MODELS};

    #[test]
    fn native_streaming_models_are_in_the_catalog() {
        let unified = AVAILABLE_MODELS
            .iter()
            .find(|model| model.id == "parakeet-unified-640ms")
            .expect("Unified model");
        assert_eq!(unified.kind, ParakeetModelKind::UnifiedEnglish640);
        assert_eq!(unified.languages, &["en"]);

        let multilingual = AVAILABLE_MODELS
            .iter()
            .find(|model| model.id == "nemotron-multilingual-1120ms")
            .expect("Nemotron multilingual model");
        assert_eq!(
            multilingual.kind,
            ParakeetModelKind::NemotronMultilingual1120
        );
        assert_eq!(multilingual.languages.first(), Some(&"en"));
        assert!(multilingual.languages.contains(&"ja"));
        assert!(multilingual.languages.contains(&"vi"));
    }
}
