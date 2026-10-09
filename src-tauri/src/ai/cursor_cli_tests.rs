use super::*;
use crate::ai::cursor_cli::CURSOR_SPEC;
#[test]
fn cursor_cold_argv_matches_the_headless_contract() {
    // Headless contract verified against `cursor-agent --help` and
    // cursor.com/docs/cli/headless: print mode + JSON output up front,
    // read-only ask mode + sandbox + trust as the isolation tail.
    let argv = cold_argv(&CURSOR_SPEC, "prompt");
    assert_eq!(
        argv,
        vec![
            "-p",
            "--output-format",
            "json",
            "--mode",
            "ask",
            "--sandbox",
            "enabled",
            "--trust",
        ]
    );
    // No model is forced: the CLI default applies unless the user picks
    // one — `--model <id>` is only emitted for an explicit selection.
    assert!(!argv.iter().any(|arg| arg == "--model"));
    // No reasoning/thinking flag exists for this CLI.
    assert!(!argv
        .iter()
        .any(|arg| arg.contains("thinking") || arg.contains("effort")));
}

#[test]
fn cursor_cold_argv_forbids_unattended_write_flags() {
    for argv in [
        cold_argv(&CURSOR_SPEC, "prompt"),
        cold_argv_for_model(&CURSOR_SPEC, "prompt", "gpt-6-luna", Default::default()),
    ] {
        for forbidden in ["--force", "-f", "--yolo", "--approve-mcps"] {
            assert!(
                !argv.iter().any(|arg| arg == forbidden),
                "cursor argv must never contain {forbidden}: {argv:?}"
            );
        }
    }
    let selected = cold_argv_for_model(&CURSOR_SPEC, "prompt", "gpt-6-luna", Default::default());
    assert!(selected
        .windows(2)
        .any(|pair| pair[0] == "--model" && pair[1] == "gpt-6-luna"));
}

#[test]
fn cursor_runtime_uses_empty_workspace() {
    let mut command = Command::new("cursor-agent");
    let isolation = apply_runtime_isolation(&CURSOR_SPEC, &mut command).expect("Cursor isolation");
    let args: Vec<OsString> = command
        .as_std()
        .get_args()
        .map(OsStr::to_os_string)
        .collect();
    assert_eq!(args.first(), Some(&OsString::from("--workspace")));
    let workspace = PathBuf::from(args.get(1).expect("workspace argument"));
    assert!(workspace.is_dir());
    // The workspace must be empty: cursor-agent would otherwise read the
    // user's project (rules, MCP config, sources) into the polish run.
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);

    drop(isolation);
    assert!(!workspace.exists());
}

#[test]
fn cursor_requires_the_headless_flags_in_help() {
    let full_help = b"  -p, --print\n  --output-format <format>\n  --mode <mode>\n  --sandbox <mode>\n  --trust\n  --workspace <path-or-name>\n  --model <model>\n";
    assert!(required_capabilities_present(&CURSOR_SPEC, full_help));
    // Missing --trust → refuse rather than hang on an interactive prompt.
    let no_trust =
        b"  --print\n  --output-format\n  --mode\n  --sandbox\n  --workspace\n  --model\n";
    assert!(!required_capabilities_present(&CURSOR_SPEC, no_trust));
    assert!(!required_capabilities_present(&CURSOR_SPEC, b""));
}

#[test]
fn cursor_result_json_parses_result_and_rejects_errors() {
    // Documented success shape (cursor.com/docs/cli/reference/output-format).
    let ok = br#"{"type":"result","subtype":"success","is_error":false,"duration_ms":1234,"result":"We should ship today.","session_id":"s"}"#;
    assert_eq!(
        parse_cli_output(OutputParser::CursorJson, ok).unwrap(),
        "We should ship today."
    );
    // Missing result → BadResponse, never typed at the cursor.
    assert!(parse_cli_output(
        OutputParser::CursorJson,
        br#"{"type":"result","subtype":"success","is_error":false}"#
    )
    .is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn cursor_fake_binary_checks_argv_workspace_and_result() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = TempDir::new().unwrap();
    let binary = fixture.path().join("cursor-agent");
    std::fs::write(
        &binary,
        r#"#!/bin/sh
[ "$1" = '-p' ] && [ "$2" = '--output-format' ] && [ "$3" = 'json' ] || exit 7
[ "$4" = '--mode' ] && [ "$5" = 'ask' ] || exit 8
[ "$6" = '--sandbox' ] && [ "$7" = 'enabled' ] && [ "$8" = '--trust' ] || exit 9
[ "$9" = '--workspace' ] || exit 10
shift 9
workspace="$1"
[ -d "$workspace" ] || exit 11
[ -z "$(ls -A "$workspace")" ] || exit 12
[ "$2" = '--model' ] && [ "$3" = 'gpt-6-luna' ] || exit 13
[ "$4" = 'um ship today' ] && [ "$#" = '4' ] || exit 14
printf '{"type":"result","is_error":false,"result":"Ship today."}\n'
"#,
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let argv = cold_argv_for_model(&CURSOR_SPEC, "Polish.", "gpt-6-luna", Default::default());
    assert_eq!(
        cold_spawn_and_collect(
            &CURSOR_SPEC,
            &binary,
            &argv,
            CURSOR_SPEC.input_mode,
            "um ship today",
            CURSOR_SPEC.output
        )
        .await
        .unwrap(),
        "Ship today."
    );
    std::fs::write(&binary, "#!/bin/sh\nprintf '{\"type\":\"result\"}\\n'\n").unwrap();
    assert!(cold_spawn_and_collect(
        &CURSOR_SPEC,
        &binary,
        &argv,
        CURSOR_SPEC.input_mode,
        "um ship today",
        CURSOR_SPEC.output
    )
    .await
    .is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn cursor_fake_models_command_has_no_invented_fallback() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = TempDir::new().unwrap();
    let binary = fixture.path().join("cursor-agent");
    std::fs::write(
        &binary,
        "#!/bin/sh\n[ \"$#\" = 1 ] && [ \"$1\" = models ] || exit 9\nprintf 'Available models\\n\\ngpt-6-luna - GPT-6 Luna\\n\\nTip: use --model <id>\\n'\n",
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let payload = run_model_list_command(&binary, &["models"]).await.unwrap();
    let models = crate::ai::cursor_cli::parse_models(&payload).unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[1].id, "gpt-6-luna");
    std::fs::write(
        &binary,
        "#!/bin/sh\nprintf 'Authentication required\\n'\nexit 1\n",
    )
    .unwrap();
    assert!(run_model_list_command(&binary, &["models"]).await.is_err());
}
