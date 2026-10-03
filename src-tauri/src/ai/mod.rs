pub mod agent_cli;
mod anthropic_current;
pub mod catalog;
pub mod contract;
pub mod error;
pub mod executor;
pub mod genai_runtime;
pub mod openai_compatible;
pub mod prompts;
pub mod providers;

pub use prompts::EnhancementOptions;

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;

pub mod polish;
pub mod polish_cli;
pub mod polish_eval;

pub mod polish_score;

pub mod output_guard;
pub mod skip;

#[cfg(test)]
mod catalog_migration_tests;

pub(crate) mod keep_words;

#[cfg(test)]
mod fix_round_tests;
pub(crate) mod settings_migration;

mod fallback_reason;

pub(crate) mod settings_selection;
