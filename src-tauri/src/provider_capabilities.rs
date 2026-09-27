//! Single source of truth for transcription engine behavior flags.
//!
//! This module is for transcription engine capabilities only. It is not UI metadata, a model
//! catalog, or formatting-LLM provider data. Context byte limits intentionally live in
//! `writing.rs`'s `ProviderContextTarget`, not here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderEngine {
    Whisper,
    Parakeet,
    Soniox,
    Openai,
    Groq,
    Deepgram,
    Cohere,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub shareable_remote: bool,
    pub supports_initial_prompt: bool,
    pub supports_structured_terms: bool,
    pub supports_vocabulary_terms: bool,
    pub supports_translate_task: bool,
}

impl ProviderEngine {
    pub fn from_engine_str(engine: &str) -> Option<Self> {
        match engine.trim().to_ascii_lowercase().as_str() {
            "whisper" => Some(Self::Whisper),
            "parakeet" => Some(Self::Parakeet),
            "soniox" => Some(Self::Soniox),
            "openai" => Some(Self::Openai),
            "groq" => Some(Self::Groq),
            "deepgram" => Some(Self::Deepgram),
            "cohere" => Some(Self::Cohere),
            "remote" => Some(Self::Remote),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Whisper => "whisper",
            Self::Parakeet => "parakeet",
            Self::Soniox => "soniox",
            Self::Openai => "openai",
            Self::Groq => "groq",
            Self::Deepgram => "deepgram",
            Self::Cohere => "cohere",
            Self::Remote => "remote",
        }
    }

    pub fn capabilities(self) -> ProviderCapabilities {
        match self {
            Self::Whisper => ProviderCapabilities {
                shareable_remote: true,
                supports_initial_prompt: true,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: true,
            },
            Self::Parakeet => ProviderCapabilities {
                shareable_remote: true,
                supports_initial_prompt: false,
                supports_structured_terms: false,
                supports_vocabulary_terms: true,
                supports_translate_task: false,
            },
            Self::Soniox => ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: false,
                supports_structured_terms: true,
                supports_vocabulary_terms: false,
                supports_translate_task: false,
            },
            Self::Openai => ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: true,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: false,
            },
            Self::Groq => ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: true,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: false,
            },
            Self::Deepgram => ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: false,
                supports_structured_terms: false,
                supports_vocabulary_terms: true,
                supports_translate_task: false,
            },
            Self::Cohere => ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: false,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: false,
            },
            Self::Remote => ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: false,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: true,
            },
        }
    }
}

pub fn capabilities_for_engine(engine: &str) -> Option<ProviderCapabilities> {
    ProviderEngine::from_engine_str(engine).map(|engine| engine.capabilities())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_engine_str_round_trips_known_ids() {
        let engines = [
            ProviderEngine::Whisper,
            ProviderEngine::Parakeet,
            ProviderEngine::Soniox,
            ProviderEngine::Openai,
            ProviderEngine::Groq,
            ProviderEngine::Deepgram,
            ProviderEngine::Cohere,
            ProviderEngine::Remote,
        ];

        for engine in engines {
            assert_eq!(
                ProviderEngine::from_engine_str(engine.as_str()),
                Some(engine)
            );
        }
    }

    #[test]
    fn from_engine_str_is_case_insensitive_and_trims() {
        assert_eq!(
            ProviderEngine::from_engine_str("Whisper"),
            Some(ProviderEngine::Whisper)
        );
        assert_eq!(
            ProviderEngine::from_engine_str(" soniox "),
            Some(ProviderEngine::Soniox)
        );
    }

    #[test]
    fn from_engine_str_returns_none_for_unknown() {
        assert_eq!(ProviderEngine::from_engine_str(""), None);
        assert_eq!(ProviderEngine::from_engine_str("foo"), None);
    }

    #[test]
    fn capabilities_match_static_truth_table() {
        assert_eq!(
            ProviderEngine::Whisper.capabilities(),
            ProviderCapabilities {
                shareable_remote: true,
                supports_initial_prompt: true,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: true,
            }
        );
        assert_eq!(
            ProviderEngine::Parakeet.capabilities(),
            ProviderCapabilities {
                shareable_remote: true,
                supports_initial_prompt: false,
                supports_structured_terms: false,
                supports_vocabulary_terms: true,
                supports_translate_task: false,
            }
        );
        assert_eq!(
            ProviderEngine::Soniox.capabilities(),
            ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: false,
                supports_structured_terms: true,
                supports_vocabulary_terms: false,
                supports_translate_task: false,
            }
        );
        assert_eq!(
            ProviderEngine::Openai.capabilities(),
            ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: true,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: false,
            }
        );
        assert_eq!(
            ProviderEngine::Groq.capabilities(),
            ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: true,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: false,
            }
        );
        assert_eq!(
            ProviderEngine::Deepgram.capabilities(),
            ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: false,
                supports_structured_terms: false,
                supports_vocabulary_terms: true,
                supports_translate_task: false,
            }
        );
        assert_eq!(
            ProviderEngine::Cohere.capabilities(),
            ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: false,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: false,
            }
        );
        assert_eq!(
            ProviderEngine::Remote.capabilities(),
            ProviderCapabilities {
                shareable_remote: false,
                supports_initial_prompt: false,
                supports_structured_terms: false,
                supports_vocabulary_terms: false,
                supports_translate_task: true,
            }
        );
    }

    #[test]
    fn capability_invariants_are_pinned() {
        let engines = [
            ProviderEngine::Whisper,
            ProviderEngine::Parakeet,
            ProviderEngine::Soniox,
            ProviderEngine::Openai,
            ProviderEngine::Groq,
            ProviderEngine::Deepgram,
            ProviderEngine::Cohere,
            ProviderEngine::Remote,
        ];

        let initial_prompt_engines: Vec<_> = engines
            .iter()
            .copied()
            .filter(|engine| engine.capabilities().supports_initial_prompt)
            .collect();
        assert_eq!(
            initial_prompt_engines,
            vec![
                ProviderEngine::Whisper,
                ProviderEngine::Openai,
                ProviderEngine::Groq
            ]
        );

        let structured_terms_engines: Vec<_> = engines
            .iter()
            .copied()
            .filter(|engine| engine.capabilities().supports_structured_terms)
            .collect();
        assert_eq!(structured_terms_engines, vec![ProviderEngine::Soniox]);

        let shareable_remote_engines: Vec<_> = engines
            .iter()
            .copied()
            .filter(|engine| engine.capabilities().shareable_remote)
            .collect();
        assert_eq!(
            shareable_remote_engines,
            vec![ProviderEngine::Whisper, ProviderEngine::Parakeet]
        );

        let vocabulary_terms_engines: Vec<_> = engines
            .iter()
            .copied()
            .filter(|engine| engine.capabilities().supports_vocabulary_terms)
            .collect();
        assert_eq!(
            vocabulary_terms_engines,
            vec![ProviderEngine::Parakeet, ProviderEngine::Deepgram]
        );

        let translate_task_engines: Vec<_> = engines
            .iter()
            .copied()
            .filter(|engine| engine.capabilities().supports_translate_task)
            .collect();
        assert_eq!(
            translate_task_engines,
            vec![ProviderEngine::Whisper, ProviderEngine::Remote]
        );
    }
}
