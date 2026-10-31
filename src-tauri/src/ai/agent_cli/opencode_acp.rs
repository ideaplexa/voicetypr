//! OpenCode 2.x single-use ACP stdio transport. No prompt-bearing argv,
//! filesystem/terminal client capabilities, or permission approvals.
use super::*;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

fn bad_response() -> MappedAiProviderError {
    MappedAiProviderError::new(AiProviderError::BadResponse)
}

async fn send(
    stdin: &mut tokio::process::ChildStdin,
    value: Value,
) -> Result<(), MappedAiProviderError> {
    let mut bytes = serde_json::to_vec(&value).map_err(|_| bad_response())?;
    bytes.push(b'\n');
    stdin.write_all(&bytes).await.map_err(|_| bad_response())?;
    stdin.flush().await.map_err(|_| bad_response())
}

struct Client {
    input: tokio::process::ChildStdin,
    output: BufReader<tokio::process::ChildStdout>,
    received: usize,
    text: String,
    session: Option<String>,
}

impl Client {
    async fn request(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, MappedAiProviderError> {
        send(
            &mut self.input,
            serde_json::json!({
                "jsonrpc": "2.0", "id": id, "method": method, "params": params
            }),
        )
        .await?;
        loop {
            // `take` bounds even a malicious/unterminated JSONL frame before
            // allocating its body. The cap covers all protocol output.
            let mut line = Vec::new();
            let remaining = MAX_CAPTURE_BYTES.saturating_sub(self.received);
            let read = (&mut self.output)
                .take((remaining + 1) as u64)
                .read_until(b'\n', &mut line)
                .await
                .map_err(|_| bad_response())?;
            self.received += read;
            if read == 0 || self.received > MAX_CAPTURE_BYTES || line.last() != Some(&b'\n') {
                return Err(bad_response());
            }
            let message: Value = serde_json::from_slice(&line).map_err(|_| bad_response())?;
            if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
                return Err(bad_response());
            }
            if let Some(method) = message.get("method").and_then(Value::as_str) {
                if let Some(request_id) = message.get("id") {
                    let reply = if method == "session/request_permission" {
                        serde_json::json!({"jsonrpc":"2.0", "id":request_id,
                            "result":{"outcome":{"outcome":"cancelled"}}})
                    } else {
                        serde_json::json!({"jsonrpc":"2.0", "id":request_id,
                            "error":{"code":-32601,"message":"Client capability unavailable"}})
                    };
                    send(&mut self.input, reply).await?;
                } else if method == "session/update" {
                    let params = &message["params"];
                    if self.session.is_none() {
                        continue;
                    }
                    if self.session.as_deref() != params["sessionId"].as_str() {
                        return Err(bad_response());
                    }
                    let update = &params["update"];
                    if update["sessionUpdate"] == "agent_message_chunk" {
                        if update["content"]["type"] != "text" {
                            return Err(bad_response());
                        }
                        self.text.push_str(
                            update["content"]["text"]
                                .as_str()
                                .ok_or_else(bad_response)?,
                        );
                    }
                }
                continue;
            }
            if message["id"] != id {
                return Err(bad_response());
            }
            if let Some(error) = message.get("error") {
                // Never surface provider errors: they may echo private input.
                // Preserve the existing one-time variant fallback using only
                // a fixed, content-free diagnostic.
                if error["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("Variant unavailable"))
                {
                    return Err(MappedAiProviderError::new(AiProviderError::AgentCli(
                        "Variant unavailable for selected model".to_string(),
                    )));
                }
                return Err(bad_response());
            }
            return message.get("result").cloned().ok_or_else(bad_response);
        }
    }
}

pub(super) async fn run(
    binary: &Path,
    argv: &[String],
    text: &str,
) -> Result<String, MappedAiProviderError> {
    let cwd = TempDir::new().map_err(|_| bad_response())?;
    let mut command = Command::new(binary);
    command
        .arg("acp")
        .current_dir(cwd.path())
        .env("PATH", resolved_path(false).await)
        .env("ZSH_TMUX_AUTOSTART", "false");
    for (name, value) in OPENCODE_V2_SPEC.static_env {
        command.env(name, value);
    }
    // session/new needs a default model before session/set_model can select
    // its variant. Only the non-content model selector enters configuration.
    if let Some(model) = argv
        .iter()
        .position(|arg| arg == "--model")
        .and_then(|i| argv.get(i + 1))
    {
        let mut config: Value =
            serde_json::from_str(OPENCODE_V2_ISOLATION_CONFIG).map_err(|_| bad_response())?;
        config["model"] = Value::String(model.split('#').next().unwrap_or(model).to_string());
        command.env("OPENCODE_CONFIG_CONTENT", config.to_string());
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    apply_no_window(&mut command);
    #[cfg(test)]
    private_input::assert_no_canaries(&command);
    let mut child = ProcessGroupGuard::new(
        command
            .group_spawn()
            .map_err(|error| map_process_failure(ProcessFailure::Spawn(error)))?,
    );
    let stderr = child.inner().stderr.take().ok_or_else(bad_response)?;
    let stderr_task = tokio::spawn(drain_bounded(stderr, MAX_CAPTURE_BYTES));
    let result = tokio::time::timeout(cold_spawn_timeout(PROVIDER_OPENCODE), async {
        let mut client = Client {
            input: child.inner().stdin.take().ok_or_else(bad_response)?,
            output: BufReader::new(child.inner().stdout.take().ok_or_else(bad_response)?),
            received: 0,
            text: String::new(),
            session: None,
        };
        let initialized = client
            .request(
                1,
                "initialize",
                serde_json::json!({
                    "protocolVersion":1, "clientCapabilities":{},
                    "clientInfo":{"name":"voicetypr","version":env!("CARGO_PKG_VERSION")}
                }),
            )
            .await?;
        if initialized["protocolVersion"] != 1 {
            return Err(bad_response());
        }
        let session = client
            .request(
                2,
                "session/new",
                serde_json::json!({
                    "cwd":cwd.path(), "mcpServers":[]
                }),
            )
            .await?;
        let session_id = session["sessionId"]
            .as_str()
            .ok_or_else(bad_response)?
            .to_string();
        client.session = Some(session_id.clone());
        if let Some(model) = argv
            .iter()
            .position(|arg| arg == "--model")
            .and_then(|i| argv.get(i + 1))
        {
            client
                .request(
                    3,
                    "session/set_model",
                    serde_json::json!({
                        "sessionId":session_id, "modelId":model
                    }),
                )
                .await?;
        }
        let response = client
            .request(
                4,
                "session/prompt",
                serde_json::json!({
                    "sessionId":session_id, "prompt":[{"type":"text","text":text}]
                }),
            )
            .await?;
        if response["stopReason"] != "end_turn" {
            return Err(bad_response());
        }
        // ACP stays open for many requests; dropping this single-use client
        // closes stdin, then the group is killed/reaped below.
        parse_plain_text(client.text.as_bytes())
    })
    .await
    .unwrap_or_else(|_| Err(MappedAiProviderError::new(AiProviderError::Timeout)));
    kill_and_reap(&mut child, None, Some(stderr_task)).await;
    result
}
