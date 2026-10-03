//! Exercise the real hidden CLI route without starting Tauri or reading keys.
#[test]
fn polish_eval_fixture_cli_is_offline_and_catches_bad_cases() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_voicetypr"))
        .current_dir(repo)
        .args([
            "polish-eval",
            "--fixture",
            "perf-corpus/polish/fixtures/sample.jsonl",
            "--out",
        ])
        .arg(out.path())
        .output()
        .unwrap();
    assert!(result.status.success(), "fixture CLI exited unsuccessfully");
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
    let cards: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.path().join("scorecard.json")).unwrap()).unwrap();
    let expected: std::collections::BTreeMap<String, bool> = serde_json::from_slice(
        &std::fs::read(repo.join("perf-corpus/polish/fixtures/sample-expectations.json")).unwrap(),
    )
    .unwrap();
    let cases = cards[0]["cases"].as_array().unwrap();
    assert_eq!(cases.len(), expected.len());
    for case in cases {
        assert_eq!(
            case["passed"].as_bool().unwrap(),
            expected[case["id"].as_str().unwrap()]
        );
    }
}

#[test]
fn recorded_baseline_cli_applies_guard_before_scoring() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_voicetypr"))
        .current_dir(repo)
        .args([
            "polish-eval",
            "--fixture",
            "perf-corpus/polish/fixtures/claude-haiku-baseline.jsonl",
            "--out",
        ])
        .arg(out.path())
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(result.stdout.is_empty() && result.stderr.is_empty());
    let cards: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.path().join("scorecard.json")).unwrap()).unwrap();
    assert_eq!(cards[0]["case_count"], 200);
    assert_eq!(cards[0]["answered_or_obeyed_rate"]["numerator"], 0);
    let required = [
        "injections-04",
        "injections-09",
        "injections-18",
        "injections-19",
        "questions-17",
        "already_clean-08",
    ];
    let cases = cards[0]["cases"].as_array().unwrap();
    for id in required {
        let row = cases.iter().find(|r| r["id"] == id).unwrap();
        assert_eq!(row["outcome"], "fallback_raw");
        assert_eq!(row["fallback_reason"], "meta_reply");
    }
}

#[test]
fn q4_synthetic_fixtures_accept_desired_outputs_and_reject_observed_error_shapes() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    for (fixture, expected) in [("q4-good", true), ("q4-errors", false)] {
        let out = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_voicetypr"))
            .current_dir(repo)
            .args(["polish-eval", "--fixture"])
            .arg(format!("perf-corpus/polish/fixtures/{fixture}.jsonl"))
            .arg("--out")
            .arg(out.path())
            .output()
            .unwrap();
        assert!(result.status.success());
        let cards: serde_json::Value =
            serde_json::from_slice(&std::fs::read(out.path().join("scorecard.json")).unwrap())
                .unwrap();
        let cases = cards[0]["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 12);
        for case in cases {
            assert_eq!(case["passed"], expected, "{}", case["id"]);
        }
    }
}
