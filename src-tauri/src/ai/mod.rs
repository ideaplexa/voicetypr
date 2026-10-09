pub mod agent_cli;
pub mod anthropic_current;
pub mod catalog;
pub mod contract;
mod cursor_cli;
pub mod error;
pub mod executor;
pub mod genai_runtime;
pub mod native_settings;
pub mod openai_compatible;
pub mod prompts;
pub mod providers;

pub use prompts::EnhancementOptions;

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;
