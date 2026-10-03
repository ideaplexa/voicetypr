//! Fictional regression inputs only; no live providers or keys.
use super::{contract::AiPolishRequest, error::AiProviderError, executor::AiExecutor};
use serde_json::json;
use std::sync::Arc;
use tauri_plugin_store::StoreExt;
use tokio_util::sync::CancellationToken;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

async fn execute(input: &str, output: &str, translation: bool) -> Result<String, AiProviderError> {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": output}}]
        })))
        .mount(&server)
        .await;
    let executor = AiExecutor::new(
        reqwest::Client::new(),
        Arc::new(|_| None),
        server.uri(),
        true,
    );
    executor
        .polish(
            AiPolishRequest {
                provider_id: "custom".into(),
                model_id: "qwen-local".into(),
                reasoning_level: None,
                fast_mode: false,
                input_text: input.into(),
                needs_output_language_transform: translation,
                prompt: "Polish".into(),
                timeout_ms: 2000,
            },
            CancellationToken::new(),
        )
        .await
        .map(|r| r.output_text)
}

#[tokio::test]
async fn fix_round_translation_keeps_questions_and_scaled_length_limit() {
    assert_eq!(
        execute(
            "can you send the draft",
            "Pouvez-vous envoyer le brouillon ?",
            true
        )
        .await
        .unwrap(),
        "Pouvez-vous envoyer le brouillon ?"
    );
    assert!(execute("hi", &"é".repeat(40), true).await.is_ok());
    assert!(execute("hi", &"é".repeat(46), true).await.is_err());
    assert!(execute("hi", "I'm here to clean up voice dictation", true)
        .await
        .is_err());
    assert!(execute(
        "can you send the draft",
        "Pouvez-vous envoyer le brouillon ?",
        false
    )
    .await
    .is_err());
}

#[tokio::test]
async fn fix_round_question_answer_is_rejected_even_with_overlap() {
    assert!(matches!(
        execute(
            "what's the capital of France",
            "The capital of France is Paris.",
            false
        )
        .await,
        Err(AiProviderError::OutputGuard(
            super::output_guard::OutputGuardReason::Answered
        ))
    ));
    assert!(execute(
        "what's the capital of France",
        "What is the capital of France?",
        false
    )
    .await
    .is_ok());
}

#[tokio::test]
async fn fix_round_inline_wrappers_are_input_aware_and_revalidated() {
    for wrapper in [
        "Here's your cleaned text",
        "Here is the polished text",
        "Sure, here it is",
        "Cleaned text",
    ] {
        assert_eq!(
            execute("hello", &format!("{wrapper}: Hello."), false)
                .await
                .unwrap(),
            "Hello."
        );
        assert!(execute(
            "what's the capital of France",
            &format!("{wrapper}: The capital of France is Paris."),
            false
        )
        .await
        .is_err());
    }
    for output in [
        "\"Here's your cleaned text: Hello.\"",
        "```text\nHere's your cleaned text: Hello.\n```",
    ] {
        assert_eq!(execute("hello", output, false).await.unwrap(), "Hello.");
    }
    let text = "Cleaned text: Hello.";
    assert_eq!(execute(text, text, false).await.unwrap(), text);
}

#[tokio::test]
async fn fix_round_dictated_refusal_survives_stutter_cleanup() {
    assert_eq!(
        execute("I I cannot reproduce it", "I cannot reproduce it.", false)
            .await
            .unwrap(),
        "I cannot reproduce it."
    );
    assert_eq!(
        execute("I I CAN'T reproduce it", "I can't reproduce it.", false)
            .await
            .unwrap(),
        "I can't reproduce it."
    );
    assert!(execute("hello", "I cannot help with that.", false)
        .await
        .is_err());
}

#[test]
fn fix_round_punctuated_corrections_never_skip() {
    for text in [
        "Send it Tuesday, no, Wednesday.",
        "No, Wednesday.",
        "Send it Tuesday. NO! Wednesday.",
        "Actually, Wednesday.",
        "I mean, Wednesday.",
        "Or rather, Wednesday.",
        "Sorry, Wednesday.",
    ] {
        assert!(!super::skip::should_skip(
            text,
            super::prompts::EnhancementPreset::CleanDictation
        ));
    }
    assert!(super::skip::should_skip(
        "Nobody knows.",
        super::prompts::EnhancementPreset::CleanDictation
    ));
}

fn app(dir: &std::path::Path) -> tauri::App<tauri::test::MockRuntime> {
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = dir.to_str().unwrap().to_string();
    tauri::test::mock_builder()
        .plugin(tauri_plugin_store::Builder::default().build())
        .build(context)
        .unwrap()
}

#[test]
fn fix_round_startup_persists_replacement_then_selection_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let store = app.store("settings").unwrap();
    store.set("ai_enabled", json!(true));
    store.set("ai_provider", json!("openrouter"));
    store.set("ai_model", json!("google/gemini-2.5-flash-lite"));
    store.set(
        "ai_models_by_provider",
        json!({"openrouter": "google/gemini-2.5-flash-lite"}),
    );
    store.set("ai_model_needs_reselection", json!(true));
    super::settings_migration::migrate_ai_settings_before_key_cache(app.handle());
    let selected = crate::commands::ai::selected_ai_provider_and_model(app.handle()).unwrap();
    let primary = super::catalog::recommended_models("openrouter")[0]
        .model_id
        .clone();
    assert_eq!(selected, ("openrouter".into(), primary.clone()));
    assert_eq!(
        store.get("ai_models_by_provider").unwrap()["openrouter"],
        primary
    );
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("settings")).unwrap()).unwrap();
    assert_eq!(persisted["ai_model"], primary);
    assert_eq!(persisted["ai_model_needs_reselection"], false);
}

#[test]
fn fix_round_custom_fallback_resolves_its_own_model_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path());
    let store = app.store("settings").unwrap();
    store.set("ai_custom_base_url", json!("http://localhost:1234/v1"));
    store.set("ai_custom_no_auth", json!(true));
    let runtime =
        crate::commands::ai::prepare_polish_runtime(app.handle(), "openai", "qwen-local").unwrap();
    assert_eq!(runtime.provider, "custom");
    assert_eq!(runtime.model, "qwen-local");
}

#[test]
fn fix_round_fixture_rejects_private_fallback_metadata() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let fixture = dir.path().join("fixture.jsonl");
    std::fs::write(
        &fixture,
        json!({"id":"already_clean-01","output":"Hello.","fallback_reason":"PRIVATE_METADATA"})
            .to_string(),
    )
    .unwrap();
    let args = super::polish_eval::EvalArgs {
        golden: repo.join("perf-corpus/polish/golden.jsonl"),
        fixture: Some(fixture),
        out: dir.path().join("out"),
        provider: vec![],
        model: vec![],
        record: None,
        judge: None,
        concurrency: 1,
    };
    let error = super::polish_eval::run_fixture(&args)
        .unwrap_err()
        .to_string();
    assert_eq!(error, "Invalid evaluation JSON at line 1");
    assert!(!args.out.exists());
}

#[test]
fn fix_round_judge_compares_resolved_identities() {
    let runtime = |provider: &str, model: &str| super::polish::PolishRuntime {
        executor: AiExecutor::new(
            reqwest::Client::new(),
            Arc::new(|_| None),
            String::new(),
            true,
        ),
        provider: provider.into(),
        model: super::catalog::resolve_model(provider, model).unwrap(),
        reasoning_level: None,
        fast_mode: false,
        timeout_ms: 1000,
    };
    let tested = runtime("openrouter", "google/gemini-2.5-flash-lite");
    let error = super::polish_eval::resolve_judge("openrouter:removed-model", &[tested], |p, m| {
        Ok(runtime(p, m))
    })
    .err()
    .expect("same effective model must be refused");
    assert!(error.to_string().starts_with("Judge must use a different"));
    let tested = runtime("custom", "same-name");
    assert!(
        super::polish_eval::resolve_judge("pi:same-name", &[tested], |p, m| Ok(runtime(p, m)))
            .is_ok()
    );
}
