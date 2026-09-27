pub use crate::provider_capabilities::{
    capabilities_for_engine, ProviderCapabilities, ProviderEngine,
};

pub fn may_translate(engine: ProviderEngine) -> bool {
    engine.capabilities().supports_translate_task
}

pub fn accepts_request_context(engine: ProviderEngine) -> bool {
    let capabilities = engine.capabilities();

    capabilities.supports_initial_prompt
        || capabilities.supports_structured_terms
        || capabilities.supports_vocabulary_terms
}

pub fn is_shareable_remote(engine: ProviderEngine) -> bool {
    engine.capabilities().shareable_remote
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_match_provider_capability_matrix() {
        let expectations = [
            (ProviderEngine::Whisper, true, true, true),
            (ProviderEngine::Parakeet, false, true, true),
            (ProviderEngine::Soniox, false, true, false),
            (ProviderEngine::Openai, false, true, false),
            (ProviderEngine::Groq, false, true, false),
            (ProviderEngine::Deepgram, false, true, false),
            (ProviderEngine::Cohere, false, false, false),
            (ProviderEngine::Remote, true, false, false),
        ];

        for (engine, expected_translate, expected_context, expected_shareable) in expectations {
            assert_eq!(
                may_translate(engine),
                expected_translate,
                "{engine:?} translate"
            );
            assert_eq!(
                accepts_request_context(engine),
                expected_context,
                "{engine:?} context"
            );
            assert_eq!(
                is_shareable_remote(engine),
                expected_shareable,
                "{engine:?} shareable"
            );
        }
    }
}
