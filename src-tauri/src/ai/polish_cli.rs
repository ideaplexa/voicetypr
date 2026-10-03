//! CLI adapters. Settings and keys are read through the existing app adapters.
use super::{
    polish::{self, PolishOutcome, PolishOutput, PolishRuntime, PolishTimings},
    prompts::{EnhancementOptions, EnhancementPreset},
};
use clap::{Args, ValueEnum};
use std::{error::Error, io::Read};
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, Copy, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Off,
    Clean,
    Writing,
    Notes,
    Message,
    Code,
}
impl Style {
    pub fn preset(self) -> EnhancementPreset {
        match self {
            Self::Off => EnhancementPreset::PersonalDictation,
            Self::Clean => EnhancementPreset::CleanDictation,
            Self::Writing => EnhancementPreset::Writing,
            Self::Notes => EnhancementPreset::Notes,
            Self::Message => EnhancementPreset::Message,
            Self::Code => EnhancementPreset::Code,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Chat,
    Email,
    Docs,
    Code,
    Terminal,
    Notes,
    Other,
}
impl Category {
    pub(crate) fn hint(self) -> Option<String> {
        use crate::writing::AppCategory as A;
        let category = match self {
            Self::Chat => A::Chat,
            Self::Email => A::Email,
            Self::Docs => A::Docs,
            Self::Code => A::Code,
            Self::Terminal => A::Terminal,
            Self::Notes => A::Notes,
            Self::Other => A::Other,
        };
        crate::writing::category_prompt_hint(category).map(|hint| {
            format!(
                "You are dictating into a {} context. {}",
                crate::writing::category_label(category),
                hint
            )
        })
    }
}

// Deliberately no Debug: --text is transcript content.
#[derive(Args)]
pub struct PolishArgs {
    #[arg(long)]
    pub text: Option<String>,
    #[arg(long, value_enum)]
    pub style: Option<Style>,
    #[arg(long, value_enum)]
    pub app_category: Option<Category>,
    #[arg(long)]
    pub provider: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub language: Option<String>,
    #[arg(long)]
    pub json: bool,
    /// Keep wording; fix only punctuation, capitals, spacing, fillers and stutters.
    #[arg(long)]
    pub keep_words: bool,
}
impl std::fmt::Debug for PolishArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PolishArgs { redacted }")
    }
}

pub(crate) fn selection(
    app: &tauri::AppHandle,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<(String, String), Box<dyn Error>> {
    let store = app
        .store("settings")
        .map_err(|_| "Cannot read Polish settings")?;
    let saved_provider = store
        .get("ai_provider")
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let provider = provider.unwrap_or(&saved_provider).to_string();
    let saved_model = if provider == saved_provider {
        store
            .get("ai_model")
            .and_then(|v| v.as_str().map(str::to_owned))
    } else {
        store
            .get("ai_models_by_provider")
            .and_then(|v| v.get(&provider)?.as_str().map(str::to_owned))
    }
    .unwrap_or_default();
    let model = model.unwrap_or(&saved_model).to_string();
    Ok((provider, model))
}

pub(crate) fn runtime(
    app: &tauri::AppHandle,
    provider: &str,
    model: &str,
) -> Result<PolishRuntime, Box<dyn Error>> {
    if super::catalog::runtime_kind(provider).is_none() {
        return Err("Select a supported Polish provider".into());
    }
    if model.trim().is_empty() && super::catalog::runtime_kind(provider) != Some("agent_cli") {
        return Err("Select a Polish model".into());
    }
    crate::commands::ai::prepare_polish_runtime(app, provider, model).map_err(|e| {
        format!(
            "Polish configuration error: {}",
            polish::error_category(&e.error)
        )
        .into()
    })
}

pub(crate) async fn run(app: &tauri::AppHandle, args: PolishArgs) -> Result<(), Box<dyn Error>> {
    let text = match args.text {
        Some(text) => text,
        None => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|_| "Cannot read UTF-8 input")?;
            text
        }
    };
    let store = app
        .store("settings")
        .map_err(|_| "Cannot read Polish settings")?;
    let enabled = store
        .get("ai_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let saved = store.get("enhancement_options");
    let options = match args.style {
        Some(style) => EnhancementOptions {
            preset: style.preset(),
        },
        None => super::prompts::enhancement_options_for_ai_enabled(saved.as_ref(), enabled)
            .map_err(|_| "Invalid saved Polish style")?,
    };
    let (provider, model) = selection(app, args.provider.as_deref(), args.model.as_deref())?;
    let result = if !options.preset.requires_ai_formatting()
        || super::skip::should_skip(&text, options.preset)
    {
        PolishOutput {
            output: text,
            outcome: PolishOutcome::Skipped,
            fallback_reason: None,
            provider,
            model,
            timings_ms: PolishTimings::default(),
        }
    } else {
        let runtime = runtime(app, &provider, &model)?;
        let settings = crate::writing::load_writing_settings(app)
            .map_err(|_| "Cannot read writing settings")?;
        let context =
            crate::writing::smart_formatting_ai_context(&settings, args.language.as_deref());
        let hint = args.app_category.and_then(Category::hint);
        polish::measure(
            &runtime,
            &text,
            &options,
            args.language.as_deref(),
            context.as_deref(),
            hint.as_deref(),
            super::keep_words::enabled(store.get("polish_keep_words").as_ref(), args.keep_words),
        )
        .await
    };
    if args.json {
        println!("{}", serde_json::to_string(&result)?);
    } else {
        print!("{}", result.output);
    }
    Ok(())
}
