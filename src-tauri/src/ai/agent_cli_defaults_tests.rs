use super::*;

fn argv(model: &str, reasoning: Option<&str>, fast: bool) -> Vec<String> {
    cold_argv_for_model_with_options(
        &CODEX_SPEC,
        "prompt",
        model,
        ClaudeCapabilities::default(),
        reasoning,
        fast,
    )
}

fn has_pair(argv: &[String], flag: &str, value: &str) -> bool {
    argv.windows(2)
        .any(|pair| pair[0] == flag && pair[1] == value)
}

#[test]
fn codex_defaults_and_explicit_overrides() {
    for model in ["", "  "] {
        let defaults = argv(model, None, true);
        assert!(has_pair(&defaults, "--model", "gpt-6-luna"));
        assert!(has_pair(
            &defaults,
            "-c",
            r#"model_reasoning_effort="medium""#
        ));
        assert!(has_pair(&defaults, "-c", r#"service_tier="fast""#));
        assert!(has_pair(&defaults, "--enable", "fast_mode"));
    }
    for (model, reasoning, fast) in [
        ("custom-model", None, true),
        ("", Some("low"), true),
        ("", None, false),
        ("custom-model", Some("low"), false),
    ] {
        let explicit = argv(model, reasoning, fast);
        assert!(has_pair(
            &explicit,
            "--model",
            if model.is_empty() {
                "gpt-6-luna"
            } else {
                model
            }
        ));
        assert!(has_pair(
            &explicit,
            "-c",
            if reasoning == Some("low") {
                r#"model_reasoning_effort="low""#
            } else {
                r#"model_reasoning_effort="medium""#
            }
        ));
        assert_eq!(has_pair(&explicit, "-c", r#"service_tier="fast""#), fast);
        assert_eq!(has_pair(&explicit, "--enable", "fast_mode"), fast);
    }
    assert!(has_pair(
        &argv("", Some("medium"), true),
        "-c",
        r#"model_reasoning_effort="medium""#
    ));
}

#[test]
fn codex_discovery_keeps_default_and_recommends_luna() {
    let models = parse_codex_models(
        br#"{"checks":{"config.load":{"details":{"model":"different-model","model_provider":"openai"}}}}"#,
    ).unwrap();
    assert!(models[0].id.is_empty());
    assert!(models[0].cli_default);
    let luna = models
        .iter()
        .find(|model| model.id == "gpt-6-luna")
        .unwrap();
    assert!(luna.recommended);
    assert!(!luna.cli_default);
}

#[test]
fn other_provider_model_and_reasoning_defaults_are_unchanged() {
    for (spec, flag, level) in [
        (&PI_SPEC, "--thinking", "off"),
        (&OMP_SPEC, "--thinking", "off"),
        (&DROID_SPEC, "--reasoning-effort", "low"),
    ] {
        let args = cold_argv_for_model(spec, "prompt", "", ClaudeCapabilities::default());
        assert!(has_pair(&args, flag, level));
        assert!(!args
            .iter()
            .any(|arg| arg == "--model" || arg == "gpt-6-luna"));
    }
    let claude = cold_argv_for_model(
        &CLAUDE_CODE_SPEC,
        "prompt",
        "",
        ClaudeCapabilities {
            effort: true,
            ..Default::default()
        },
    );
    assert!(has_pair(&claude, "--model", "sonnet"));
    assert!(has_pair(&claude, "--effort", "low"));
}
