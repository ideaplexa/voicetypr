//! Owner-only local IPC. Only the desktop binds; CLI invocations never spawn a
//! second warm owner. Content is bounded, in memory, and never logged.
use super::*;
static DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static STOP: LazyLock<tokio_util::sync::CancellationToken> = LazyLock::new(tokio_util::sync::CancellationToken::new);
pub(super) fn stop() { STOP.cancel(); }

pub(super) fn configure(base: &Path) { let _ = DIRECTORY.set(base.join("warm-agent")); }

pub(super) fn start(app: tauri::AppHandle) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let Some(directory) = DIRECTORY.get() else { return; };
        if std::fs::create_dir_all(directory).is_err() { return; }
        if !std::fs::symlink_metadata(directory).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) { return; }
        if std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).is_err() { return; }
        let socket = directory.join("owner.sock");
        // Do not steal a live owner's address. A dead socket has no child behind it.
        if std::os::unix::net::UnixStream::connect(&socket).is_ok() { return; }
        let _ = std::fs::remove_file(&socket);
        tauri::async_runtime::spawn(async move {
            let Ok(listener) = tokio::net::UnixListener::bind(&socket) else { return; };
            loop {
                let accepted = tokio::select! {
                    biased;
                    _ = STOP.cancelled() => break,
                    accepted = listener.accept() => accepted,
                };
                let Ok((stream, _)) = accepted else { break; };
                // Bound pending requests as well as each request's bytes and time.
                if let Ok(permit) = CONNECTIONS.clone().try_acquire_owned() {
                    let app = app.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let _ = tokio::time::timeout(Duration::from_secs(25), serve(stream, app)).await;
                    });
                }
            }
            let _ = std::fs::remove_file(&socket);
        });
    }
    #[cfg(not(unix))]
    let _ = app;
}

#[cfg(unix)]
static CONNECTIONS: LazyLock<std::sync::Arc<Semaphore>> = LazyLock::new(|| std::sync::Arc::new(Semaphore::new(4)));

#[cfg(unix)]
async fn read_frame<R: tokio::io::AsyncRead + Unpin>(reader: &mut R) -> Option<serde_json::Value> {
    use tokio::io::AsyncReadExt;
    let size = reader.read_u32().await.ok()? as usize;
    if size > MAX_CAPTURE_BYTES * 2 { return None; }
    let mut data = vec![0; size];
    reader.read_exact(&mut data).await.ok()?;
    serde_json::from_slice(&data).ok()
}
#[cfg(unix)]
async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(writer: &mut W, value: &serde_json::Value) -> Option<()> {
    let bytes = serde_json::to_vec(value).ok()?;
    if bytes.len() > MAX_CAPTURE_BYTES * 2 { return None; }
    writer.write_u32(bytes.len() as u32).await.ok()?;
    writer.write_all(&bytes).await.ok()?;
    writer.flush().await.ok()
}
#[cfg(unix)]
async fn serve(mut stream: tokio::net::UnixStream, app: tauri::AppHandle) -> Option<()> {
    let request = read_frame(&mut stream).await?;
    let result = match request["command"].as_str()? {
        "status" => warm_session::status(),
        "warm" => {
            crate::commands::ai_lifecycle::warm_selected(app).await;
            warm_session::status()
        }
        "polish" => {
            let provider = request["provider"].as_str()?;
            let model = request["model"].as_str()?;
            if !warm_session::selected(provider, model) { return None; }
            let spec = spec_for(provider)?;
            let binary = resolve_binary(spec.binary).await?;
            let (_, help, _) = discover_capabilities_for_polish(&binary, spec, false).await;
            let argv: Vec<String> = serde_json::from_value(request["argv"].clone()).ok()?;
            let input = request["input"].as_str()?;
            let budget = Duration::from_millis(request["budget_ms"].as_u64()?.min(10_000));
            use tokio::io::AsyncReadExt;
            let attempt = tokio::select! {
                biased;
                _ = stream.read_u8() => return None,
                result = warm_session::run(provider, model, &binary, &argv, input, &help, budget) => result,
            };
            match attempt {
                Some(Ok(output)) => {
                    warm_session::replenish(provider, model, &binary, &argv, &help);
                    serde_json::json!({"output":output})
                }
                _ => serde_json::json!({"fallback":true}),
            }
        }
        _ => return None,
    };
    write_frame(&mut stream, &result).await
}

pub(super) async fn request(value: serde_json::Value, budget: Duration) -> Option<serde_json::Value> {
    #[cfg(unix)]
    {
        let socket = DIRECTORY.get()?.join("owner.sock");
        return tokio::time::timeout(budget, async {
            let mut stream = tokio::net::UnixStream::connect(socket).await.ok()?;
            write_frame(&mut stream, &value).await?;
            read_frame(&mut stream).await
        }).await.ok().flatten();
    }
    #[cfg(not(unix))]
    { let _ = (value, budget); None }
}

pub(super) async fn polish(provider: &str, model: &str, _binary: &Path, argv: &[String], input: &str, budget: Duration) -> Option<Result<String, MappedAiProviderError>> {
    let value = request(serde_json::json!({"command":"polish", "provider":provider, "model":model, "argv":argv, "input":input, "budget_ms":budget.as_millis() as u64}), budget).await?;
    value["output"].as_str().map(|text| Ok(text.to_owned()))
}

#[derive(clap::Args, Debug)]
pub struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommand,
}
#[derive(clap::Subcommand, Debug)]
enum AgentCommand {
    Warm { #[arg(long, required = true)] selected: bool, #[arg(long)] json: bool },
    Status { #[arg(long)] json: bool },
}
impl AgentArgs {
    pub(crate) fn wants_json(&self) -> bool {
        match self.command { AgentCommand::Warm { json, .. } | AgentCommand::Status { json } => json }
    }
}
pub(crate) async fn run(args: AgentArgs) -> Result<(), Box<dyn std::error::Error>> {
    let command = match args.command { AgentCommand::Warm { .. } => "warm", AgentCommand::Status { .. } => "status" };
    let status = request(serde_json::json!({"command":command}), Duration::from_secs(20)).await
        .unwrap_or_else(|| serde_json::json!({"status":if cfg!(unix) {"desktop_unavailable"} else {"unsupported"}}));
    if args.wants_json() { println!("{}", status); }
    else { println!("{}", status["status"].as_str().unwrap_or("unavailable")); }
    Ok(())
}
