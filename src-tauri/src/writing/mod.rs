mod app_category;
mod library_rules;
mod pipeline;
mod settings;
mod vocabulary;

#[allow(unused_imports)]
pub use app_category::{category_label, category_prompt_hint, classify, AppCategory};
#[allow(unused_imports)]
pub use pipeline::{
    capture_active_app_context, effective_personal_dictation_mode, effective_pipeline_config,
    process_transcription, resolve_pipeline_config, PipelineAiState,
};
#[allow(unused_imports)]
pub use settings::{
    load_writing_settings, sanitize_writing_settings, save_writing_settings, AiExecutionMetadata,
    AppFormattingRule, AppliedWritingOperation, ContextHint, CustomWord, Snippet,
    TextReplacementRule, WritingError, WritingOperationKind, WritingProfile, WritingResult,
    WritingSettings, WritingStageTimings, WritingWarning,
};
#[allow(unused_imports)]
pub use vocabulary::{
    compile_context_for_target, compile_deepgram_keyterms, compile_parakeet_custom_vocabulary,
    compile_soniox_context, ProviderContextCapabilities, ProviderContextTarget, SonioxContext,
    SonioxContextField,
};

#[allow(unused_imports)]
pub(crate) use library_rules::{
    apply_final_restoration_guard, apply_library_rules, sanitize_transcript,
};
#[allow(unused_imports)]
pub(crate) use pipeline::{
    language_scope_matches, normalize_language_scope, smart_formatting_ai_context,
};
