//! The supported pi RPC contract is checked before every dictation. All
//! conversations are ephemeral; reset is verified both before and after a take.
use super::*;
use tokio::io::{AsyncBufReadExt, BufReader};

pub(super) struct PiRpc {
    pub(super) binary: PathBuf,
    pub(super) argv: Vec<String>,
    child: ProcessGroupGuard,
    lines: BufReader<tokio::process::ChildStdout>,
    stderr: BoundedReadTask,
    prompt: memory_prompt::MemoryPrompt,
    _cwd: TempDir,
}

impl PiRpc {
    pub(super) async fn spawn(binary: &Path, argv: &[String], help: &[u8]) -> Result<Self, MappedAiProviderError> {
        for flag in ["--mode", "--no-session", "--no-mcp", "--offline"] {
            if !help_advertises_flag(help, flag) { return Err(failure()); }
        }
        let cwd = TempDir::new().map_err(|_| failure())?;
        let mut args = Vec::new();
        let mut prompt = None;
        let mut index = 0;
        while index < argv.len() {
            match argv[index].as_str() {
                "-p" | "--print" => index += 1,
                "--system-prompt" => {
                    prompt = argv.get(index + 1).map(String::as_str);
                    index += 2;
                }
                "--mode" => index += 2,
                _ => { args.push(argv[index].clone()); index += 1; }
            }
        }
        let mut command = Command::new(binary);
        command.args(args).args(["--mode", "rpc", "--no-mcp", "--offline"]);
        command.current_dir(cwd.path());
        command.env("PATH", resolved_path(false).await);
        command.env("ZSH_TMUX_AUTOSTART", "false");
        command.env("PI_TELEMETRY", "0");
        let memory = memory_prompt::MemoryPrompt::attach(&mut command, prompt.ok_or_else(failure)?, "--system-prompt")?;
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        apply_no_window(&mut command);
        let initial_feed = memory.feed();
        let mut child = ProcessGroupGuard::new(command.group_spawn().map_err(|_| failure())?);
        let stdout = child.inner().stdout.take().ok_or_else(failure)?;
        let stderr = child.inner().stderr.take().ok_or_else(failure)?;
        let mut rpc = Self {
            binary: binary.to_owned(), argv: argv.to_vec(), child,
            lines: BufReader::new(stdout),
            stderr: tokio::spawn(drain_bounded(stderr, MAX_CAPTURE_BYTES)), _cwd: cwd, prompt: memory,
        };
        let mut remaining = MAX_CAPTURE_BYTES;
        rpc.command("get_state", &mut remaining).await?;
        initial_feed.await.map_err(|_| failure())?;
        rpc.reset().await?;
        Ok(rpc)
    }

    async fn send(&mut self, value: serde_json::Value) -> Result<(), MappedAiProviderError> {
        let mut data = serde_json::to_vec(&value).map_err(|_| failure())?;
        if data.len() > MAX_CAPTURE_BYTES { return Err(failure()); }
        data.push(b'\n');
        let stdin = self.child.inner().stdin.as_mut().ok_or_else(failure)?;
        stdin.write_all(&data).await.map_err(|_| failure())?;
        stdin.flush().await.map_err(|_| failure())
    }

    async fn event(&mut self, remaining: &mut usize) -> Result<serde_json::Value, MappedAiProviderError> {
        // fill_buf/consume caps even an unterminated malicious line, before allocating it.
        let mut line = Vec::new();
        loop {
            let buffer = self.lines.fill_buf().await.map_err(|_| failure())?;
            if buffer.is_empty() { return Err(failure()); }
            let count = buffer.iter().position(|b| *b == b'\n').map_or(buffer.len(), |n| n + 1);
            if count > *remaining { return Err(failure()); }
            *remaining -= count;
            line.extend_from_slice(&buffer[..count]);
            let complete = buffer[count - 1] == b'\n';
            self.lines.consume(count);
            if complete { return serde_json::from_slice(&line).map_err(|_| failure()); }
        }
    }

    async fn command(&mut self, kind: &str, remaining: &mut usize) -> Result<serde_json::Value, MappedAiProviderError> {
        self.send(serde_json::json!({"id":"control", "type":kind})).await?;
        loop {
            let event = self.event(remaining).await?;
            if event["type"] == "response" && event["id"] == "control" {
                if event["command"] != kind || event["success"] != true { return Err(failure()); }
                return Ok(event["data"].clone());
            }
        }
    }

    async fn reset(&mut self) -> Result<(), MappedAiProviderError> {
        let mut remaining = MAX_CAPTURE_BYTES;
        let feed = self.prompt.feed();
        let reset = self.command("new_session", &mut remaining).await?;
        if reset["cancelled"] != false { return Err(failure()); }
        let state = self.command("get_state", &mut remaining).await?;
        feed.await.map_err(|_| failure())?;
        if !state["sessionFile"].is_null() || state["isStreaming"] != false { return Err(failure()); }
        let messages = self.command("get_messages", &mut remaining).await?;
        if !messages["messages"].as_array().is_some_and(Vec::is_empty) { return Err(failure()); }
        Ok(())
    }

    pub(super) async fn run(&mut self, text: &str) -> Result<String, MappedAiProviderError> {
        self.reset().await?;
        self.send(serde_json::json!({"id":"take", "type":"prompt", "message":text})).await?;
        let mut remaining = MAX_CAPTURE_BYTES;
        let mut output = None;
        loop {
            let event = self.event(&mut remaining).await?;
            if event["type"] == "response" && event["success"] == false { return Err(failure()); }
            if event["type"] == "message_end" && event["message"]["role"] == "assistant" {
                let message = &event["message"];
                if message["stopReason"] == "error" || message["stopReason"] == "aborted" { return Err(failure()); }
                let content = message["content"].as_array().ok_or_else(failure)?;
                let mut text = String::new();
                for block in content {
                    match block["type"].as_str() {
                        Some("text") => text.push_str(block["text"].as_str().ok_or_else(failure)?),
                        Some("thinking") => {},
                        _ => return Err(failure()),
                    }
                }
                output = Some(text);
            }
            if event["type"] == "agent_end" { break; }
        }
        let output = output.filter(|text| !text.trim().is_empty()).ok_or_else(failure)?;
        // Do not keep the previous transcript resident while idle.
        self.reset().await?;
        Ok(output)
    }
}

impl Drop for PiRpc {
    fn drop(&mut self) {
        self.stderr.abort();
        // ProcessGroupGuard owns tree kill/reap, including cancellation and panic.
    }
}
fn failure() -> MappedAiProviderError { MappedAiProviderError::new(AiProviderError::BadResponse) }

