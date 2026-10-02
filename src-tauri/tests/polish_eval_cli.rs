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
