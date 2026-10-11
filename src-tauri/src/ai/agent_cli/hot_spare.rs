//! A prepared Claude process is consumed once, never a reusable conversation.

use parking_lot::Mutex;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Instant;

use super::*;

const IDLE_EXPIRY: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Default)]
pub(super) struct HotSpare {
    shared: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    generation: u64,
    spare: Option<Prepared>,
}

struct Prepared {
    binary_path: PathBuf,
    argv: Vec<String>,
    created: Instant,
    child: Option<ProcessGroupGuard>,
    stdout: Option<tokio::task::JoinHandle<Result<String, MappedAiProviderError>>>,
    stderr: Option<BoundedReadTask>,
    cwd: Option<TempDir>,
    isolation: Option<RuntimeIsolation>,
}

impl HotSpare {
    pub(super) fn generation(&self) -> u64 {
        self.shared.lock().generation
    }

    #[cfg(test)]
    pub(super) async fn warm(&self, binary_path: &Path, argv: &[String]) {
        self.warm_if_current(binary_path, argv, self.generation())
            .await;
    }

    pub(super) async fn warm_if_current(
        &self,
        binary_path: &Path,
        argv: &[String],
        expected_generation: u64,
    ) {
        let generation = {
            let mut state = self.shared.lock();
            if state.generation != expected_generation {
                return;
            }
            // Recording-time language may still be unknown. Preserve the exact
            // last-request spare; settings changes explicitly invalidate it.
            if state
                .spare
                .as_ref()
                .is_some_and(|spare| spare.created.elapsed() < IDLE_EXPIRY)
            {
                return;
            }
            state.generation += 1;
            if state.spare.take().is_some() {
                log::info!("Claude hot spare invalidated");
            }
            state.generation
        };
        let Ok(spare) = Prepared::spawn(binary_path, argv).await else {
            return;
        };
        let expires = spare.created + IDLE_EXPIRY;
        {
            let mut state = self.shared.lock();
            if state.generation != generation {
                log::info!("Claude hot spare invalidated");
                return;
            }
            state.spare = Some(spare);
            log::info!("Claude hot spare prepared");
        }
        let shared = Arc::downgrade(&self.shared);
        tokio::spawn(async move {
            tokio::time::sleep_until(expires).await;
            if let Some(shared) = shared.upgrade() {
                let mut state = shared.lock();
                if state.generation == generation {
                    state.generation += 1;
                    if state.spare.take().is_some() {
                        log::info!("Claude hot spare expired");
                    }
                }
            }
        });
    }

    /// Synchronously kills the idle group and prevents pending spawns publishing.
    pub(super) fn invalidate(&self) {
        let mut state = self.shared.lock();
        state.generation += 1;
        if state.spare.take().is_some() {
            log::info!("Claude hot spare invalidated");
        }
    }

    pub(super) async fn take_and_run(
        &self,
        binary_path: &Path,
        argv: &[String],
        text: &str,
    ) -> Option<Result<String, MappedAiProviderError>> {
        let spare = {
            let mut state = self.shared.lock();
            state.generation += 1;
            let spare = state.spare.take()?;
            if !spare.matches(binary_path, argv) {
                log::info!("Claude hot spare mismatch");
                return None;
            }
            if spare.created.elapsed() >= IDLE_EXPIRY {
                log::info!("Claude hot spare expired");
                return None;
            }
            log::info!("Claude hot spare consumed");
            spare
        };
        // Reserve half the executor's existing budget for the caller's cold fallback.
        Some(
            spare
                .run(text, cold_spawn_timeout(PROVIDER_CLAUDE_CODE) / 2)
                .await,
        )
    }
}

impl Prepared {
    fn matches(&self, binary_path: &Path, argv: &[String]) -> bool {
        self.binary_path == binary_path && self.argv == argv
    }

    async fn spawn(binary_path: &Path, argv: &[String]) -> Result<Self, MappedAiProviderError> {
        let mut stream_argv = argv.to_vec();
        for index in 0..stream_argv.len().saturating_sub(1) {
            if stream_argv[index] == "--output-format" && stream_argv[index + 1] == "json" {
                stream_argv[index + 1] = "stream-json".to_string();
            }
        }
        stream_argv.extend(["--input-format", "stream-json", "--verbose"].map(str::to_string));
        let cwd = TempDir::new().map_err(|error| map_process_failure(ProcessFailure::Io(error)))?;
        let mut command = Command::new(binary_path);
        command.args(&stream_argv);
        let isolation = apply_runtime_isolation(&CLAUDE_CODE_SPEC, &mut command)?;
        command.current_dir(cwd.path());
        command.env("PATH", resolved_path(false).await);
        command.env("ZSH_TMUX_AUTOSTART", "false");
        for (name, value) in CLAUDE_CODE_SPEC.static_env {
            command.env(name, value);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.kill_on_drop(true);
        apply_no_window(&mut command);
        let child = ProcessGroupGuard::new(
            command
                .group_spawn()
                .map_err(|error| map_process_failure(ProcessFailure::Spawn(error)))?,
        );
        // Establish ownership before extracting pipes so every failure kills and reaps.
        let mut spare = Self {
            binary_path: binary_path.to_path_buf(),
            argv: argv.to_vec(),
            created: Instant::now(),
            child: Some(child),
            stdout: None,
            stderr: None,
            cwd: Some(cwd),
            isolation: Some(isolation),
        };
        let child = spare.child.as_mut().unwrap();
        let stdout = child.inner().stdout.take().ok_or_else(bad_response)?;
        let stderr = child.inner().stderr.take().ok_or_else(bad_response)?;
        spare.stdout = Some(tokio::spawn(read_result(stdout)));
        spare.stderr = Some(tokio::spawn(drain_bounded(stderr)));
        Ok(spare)
    }

    async fn run(mut self, text: &str, timeout: Duration) -> Result<String, MappedAiProviderError> {
        let deadline = Instant::now() + timeout;
        let result = tokio::time::timeout_at(deadline, async {
            let input = serde_json::json!({
                "type": "user",
                "message": {"role": "user", "content": text}
            });
            let mut bytes = serde_json::to_vec(&input).map_err(|_| bad_response())?;
            bytes.push(b'\n');
            // Keep stdin open: EOF can make Claude exit before returning its result.
            let child = self.child.as_mut().unwrap();
            let stdin = child.inner().stdin.as_mut().ok_or_else(bad_response)?;
            stdin
                .write_all(&bytes)
                .await
                .map_err(|error| map_process_failure(ProcessFailure::Io(error)))?;
            stdin
                .flush()
                .await
                .map_err(|error| map_process_failure(ProcessFailure::Io(error)))?;
            let result = self
                .stdout
                .as_mut()
                .unwrap()
                .await
                .map_err(|_| bad_response())?;
            self.stdout.take();
            if child
                .inner()
                .try_wait()
                .map_err(|error| map_process_failure(ProcessFailure::Io(error)))?
                .is_some_and(|status| !status.success())
            {
                return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
                    "The prepared CLI exited unsuccessfully.".to_string(),
                )));
            }
            result
        })
        .await
        .unwrap_or_else(|_| Err(MappedAiProviderError::new(AiProviderError::Timeout)));
        // Cleanup shares the request deadline; Drop still owns cleanup on cancellation.
        let child = self.child.as_mut().unwrap();
        let kill_result = child.start_kill();
        if matches!(
            tokio::time::timeout_at(deadline, child.wait()).await,
            Ok(Ok(_))
        ) && group_kill_is_settled(&kill_result)
        {
            child.disarm();
            self.child.take();
        }
        result
    }
}

impl Drop for Prepared {
    fn drop(&mut self) {
        for task in [self.stderr.take()].into_iter().flatten() {
            task.abort();
        }
        if let Some(task) = self.stdout.take() {
            task.abort();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let cwd = self.cwd.take();
            let isolation = self.isolation.take();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = terminate_process_group(&mut child).await;
                    drop((cwd, isolation));
                });
            }
        }
    }
}

fn bad_response() -> MappedAiProviderError {
    MappedAiProviderError::new(AiProviderError::BadResponse)
}

/// Byte-based JSONL framing caps the entire conversation, including a line
/// without a newline. Assistant/system events never become polished text.
async fn read_result(
    mut stdout: tokio::process::ChildStdout,
) -> Result<String, MappedAiProviderError> {
    let mut line = Vec::new();
    let mut captured = 0usize;
    let mut chunk = [0u8; 8192];
    loop {
        let read = stdout
            .read(&mut chunk)
            .await
            .map_err(|error| map_process_failure(ProcessFailure::Io(error)))?;
        captured += read;
        if captured > MAX_CAPTURE_BYTES {
            return Err(bad_response());
        }
        for &byte in &chunk[..read] {
            if byte == b'\n' {
                if let Some(result) = parse_result_line(&line) {
                    return result;
                }
                line.clear();
            } else {
                line.push(byte);
            }
        }
        if read == 0 {
            return parse_result_line(&line).unwrap_or_else(|| Err(bad_response()));
        }
    }
}

fn parse_result_line(line: &[u8]) -> Option<Result<String, MappedAiProviderError>> {
    let value: Value = serde_json::from_slice(line).ok()?;
    (value.get("type").and_then(Value::as_str) == Some("result")).then(|| parse_claude_json(line))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake_binary(script: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("claude");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, path)
    }

    fn argv() -> Vec<String> {
        [
            "--print",
            "--output-format",
            "json",
            "--model",
            "claude-haiku-5-5",
            "--effort",
            "medium",
            "--system-prompt",
            "polish",
        ]
        .map(str::to_string)
        .to_vec()
    }

    #[tokio::test]
    async fn consumed_spares_are_fresh_isolated_conversations() {
        let (_dir, binary) = fake_binary(
            r#"
[ "$(find . -mindepth 1 | wc -l | tr -d ' ')" = 0 ] || exit 9
count=0
while IFS= read -r input; do
    count=$((count + 1))
    printf '{"type":"assistant","content":"ignore me"}\n'
    printf '{"type":"result","result":"%s:%s"}\n' "$$" "$count"
done
"#,
        );
        let spare = HotSpare::default();
        let args = argv();
        spare.warm(&binary, &args).await;
        let first = spare
            .take_and_run(&binary, &args, "one")
            .await
            .unwrap()
            .unwrap();
        assert!(first.ends_with(":1"));
        assert!(spare.take_and_run(&binary, &args, "two").await.is_none());
        spare.warm(&binary, &args).await;
        let second = spare
            .take_and_run(&binary, &args, "two")
            .await
            .unwrap()
            .unwrap();
        assert!(second.ends_with(":1"));
        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn stream_flags_preserve_cold_arguments_and_encode_input() {
        let (_dir, binary) = fake_binary(
            r#"
[ "$1" = --print ] && [ "$2" = --output-format ] && [ "$3" = stream-json ] || exit 9
[ "$4" = --model ] && [ "$5" = claude-haiku-5-5 ] && [ "$6" = --effort ] && [ "$7" = medium ] || exit 9
[ "$8" = --system-prompt ] && [ "$9" = polish ] || exit 9
shift 9
[ "$1" = --input-format ] && [ "$2" = stream-json ] && [ "$3" = --verbose ] || exit 9
IFS= read -r input
case "$input" in *'"type":"user"'* ) ;; *) exit 9 ;; esac
printf '{"type":"result","result":"ok"}\n'
"#,
        );
        let spare = HotSpare::default();
        let args = argv();
        spare.warm(&binary, &args).await;
        assert_eq!(
            spare
                .take_and_run(&binary, &args, "quoted \" text\nline")
                .await
                .unwrap()
                .unwrap(),
            "ok"
        );
    }

    #[tokio::test]
    async fn generic_prefetch_preserves_exact_prompt_and_requests_still_match_exactly() {
        let (_dir, binary) =
            fake_binary("IFS= read -r input\nprintf '{\"type\":\"result\",\"result\":\"ok\"}\\n'");
        let spare = HotSpare::default();
        let mut exact = argv();
        *exact.last_mut().unwrap() = "polish in delivered language".to_string();
        let generic = argv();
        spare.warm(&binary, &exact).await;
        let generation = spare.generation();
        spare.warm(&binary, &generic).await;
        assert_eq!(spare.generation(), generation);
        assert!(spare
            .shared
            .lock()
            .spare
            .as_ref()
            .unwrap()
            .matches(&binary, &exact));
        assert_eq!(
            spare
                .take_and_run(&binary, &exact, "text")
                .await
                .unwrap()
                .unwrap(),
            "ok"
        );

        spare.warm(&binary, &exact).await;
        spare.warm(&binary, &generic).await;
        assert!(spare
            .take_and_run(&binary, &generic, "text")
            .await
            .is_none());
        assert!(spare.shared.lock().spare.is_none());
        assert!(spare.take_and_run(&binary, &exact, "text").await.is_none());
    }

    #[tokio::test]
    async fn mismatches_invalidation_and_expiration_discard_spares() {
        let (_dir, binary) =
            fake_binary("IFS= read -r input\nprintf '{\"type\":\"result\",\"result\":\"ok\"}\\n'");
        let spare = HotSpare::default();
        let args = argv();
        spare.warm(&binary, &args).await;
        let mut changed = args.clone();
        changed.push("--different-capability".to_string());
        assert!(spare
            .take_and_run(&binary, &changed, "text")
            .await
            .is_none());
        assert!(spare.take_and_run(&binary, &args, "text").await.is_none());
        spare.warm(&binary, &args).await;
        spare.clone().invalidate();
        assert!(spare.take_and_run(&binary, &args, "text").await.is_none());
        spare.warm(&binary, &args).await;
        spare.shared.lock().spare.as_mut().unwrap().created = Instant::now() - IDLE_EXPIRY;
        assert!(spare.take_and_run(&binary, &args, "text").await.is_none());
        spare.warm(&binary, &args).await;
        assert!(spare
            .take_and_run(&binary.with_extension("other"), &args, "text")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn crash_timeout_and_oversized_output_return_errors() {
        for (script, times_out) in [
            ("IFS= read -r input\nexit 7", false),
            ("IFS= read -r input\nexec sleep 30", true),
            ("IFS= read -r input\nhead -c 140000 /dev/zero", false),
        ] {
            let (_dir, binary) = fake_binary(script);
            let spare = Prepared::spawn(&binary, &argv()).await.unwrap();
            let error = spare.run("text", Duration::from_secs(1)).await.unwrap_err();
            if times_out {
                assert!(matches!(error.error, AiProviderError::Timeout));
            } else {
                assert!(!matches!(error.error, AiProviderError::Timeout));
            }
        }
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn invalidation_rejects_pending_and_pre_discovery_generations() {
        let (_dir, binary) = fake_binary("IFS= read -r input");
        let spare = HotSpare::default();
        let args = argv();
        let generation = spare.generation();
        spare.invalidate();
        spare.warm_if_current(&binary, &args, generation).await;
        assert!(spare.shared.lock().spare.is_none());

        // Hold the existing async PATH cache lock to suspend warming before spawn.
        let cache = RESOLVED_PATH.get_or_init(|| tokio::sync::Mutex::new(None));
        let cache_guard = cache.lock().await;
        let warm = spare.warm(&binary, &args);
        tokio::pin!(warm);
        tokio::select! {
            biased;
            _ = &mut warm => panic!("warm should wait for the PATH cache"),
            _ = std::future::ready(()) => {}
        }
        spare.invalidate();
        drop(cache_guard);
        warm.await;
        assert!(spare.shared.lock().spare.is_none());
    }

    #[tokio::test]
    async fn hot_error_allows_fresh_cold_fallback_with_original_text() {
        let (_dir, binary) = fake_binary(
            r#"
if [ "$3" = stream-json ]; then
    IFS= read -r input
    printf '{"type":"result","is_error":true,"result":"hot failure"}\n'
else
    input=$(cat)
    [ "$input" = 'original text' ] || exit 9
    printf '{"type":"result","result":"cold success"}\n'
fi
"#,
        );
        let spare = HotSpare::default();
        let args = argv();
        spare.warm(&binary, &args).await;
        assert!(spare
            .take_and_run(&binary, &args, "original text")
            .await
            .unwrap()
            .is_err());
        let result = cold_spawn_and_collect(
            &CLAUDE_CODE_SPEC,
            &binary,
            &args,
            InputMode::Stdin,
            "original text",
            OutputParser::ClaudeJson,
        )
        .await
        .unwrap();
        assert_eq!(result, "cold success");
        assert!(spare
            .take_and_run(&binary, &args, "next text")
            .await
            .is_none());
    }

    #[tokio::test]
    async fn dropping_request_future_kills_descendants() {
        let markers = TempDir::new().unwrap();
        let ready = markers.path().join("ready");
        let survivor = markers.path().join("survivor");
        let (_dir, binary) = fake_binary(&format!(
            "(sleep 1; printf survived > '{}') &\nprintf ready > '{}'\nIFS= read -r input\nwait",
            survivor.display(),
            ready.display(),
        ));
        let spare = HotSpare::default();
        let args = argv();
        spare.warm(&binary, &args).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        {
            let run = spare.take_and_run(&binary, &args, "text");
            tokio::pin!(run);
            tokio::select! {
                biased;
                _ = &mut run => panic!("fake CLI should remain pending"),
                _ = std::future::ready(()) => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(
            !survivor.exists(),
            "cancelled CLI descendant must not survive"
        );
        assert!(spare.shared.lock().spare.is_none());
    }

    #[test]
    fn only_result_events_are_parsed_and_provider_errors_are_preserved() {
        assert!(parse_result_line(br#"{"type":"assistant","content":"wrong"}"#).is_none());
        assert!(
            parse_result_line(br#"{"type":"result","is_error":true,"result":"failure"}"#)
                .unwrap()
                .is_err()
        );
    }
}
