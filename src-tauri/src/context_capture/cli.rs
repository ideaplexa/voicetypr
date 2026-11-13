//! Diagnostic output is metadata-only unless the caller explicitly includes text.
use super::{Status, BUDGET};
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub(crate) struct ContextArgs {
    #[command(subcommand)]
    command: Command,
}
#[derive(Debug, Subcommand)]
enum Command {
    Capture(CaptureArgs),
}
#[derive(Debug, Args)]
struct CaptureArgs {
    /// Capture only; never insert text or invoke a Polish provider.
    #[arg(long, required = true)]
    dry_run: bool,
    #[arg(long)]
    json: bool,
    /// Explicitly expose the ephemeral reference to stdout only.
    #[arg(long)]
    include_text: bool,
    /// Wait after capture before validating the target again.
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u64).range(0..=60_000))]
    delay_ms: u64,
    /// Apply the stricter local-provider reference limits.
    #[arg(long)]
    local: bool,
}
impl ContextArgs {
    pub(crate) fn wants_json(&self) -> bool {
        match &self.command {
            Command::Capture(a) => a.json,
        }
    }
}
#[derive(serde::Serialize)]
struct Metadata {
    status: Status,
    elapsed_ms: u64,
    before_characters: usize,
    selection_characters: usize,
}
pub(crate) async fn run(args: &ContextArgs) -> Result<(), &'static str> {
    let Command::Capture(args) = &args.command;
    let started = std::time::Instant::now();
    let mut status = if cfg!(target_os = "macos") {
        Status::Timeout
    } else {
        Status::Unsupported
    };
    let mut reference: Option<crate::ai::context_reference::ContextReference> = None;
    #[cfg(target_os = "macos")]
    {
        if let Some(worker) = super::Worker::claim() {
            let deadline = started + BUDGET;
            let local = args.local;
            let captured = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                tauri::async_runtime::spawn_blocking(move || {
                    let _worker = worker;
                    super::macos::capture(
                        local,
                        deadline,
                        &std::sync::atomic::AtomicBool::new(false),
                    )
                }),
            )
            .await;
            match captured {
                Ok(Ok(Ok(captured))) => {
                    tokio::time::sleep(std::time::Duration::from_millis(args.delay_ms)).await;
                    if let Some(worker) = super::Worker::claim() {
                        let deadline = std::time::Instant::now() + BUDGET;
                        let validated = tokio::time::timeout_at(
                            tokio::time::Instant::from_std(deadline),
                            tauri::async_runtime::spawn_blocking(move || {
                                let _worker = worker;
                                super::macos::validate(&captured.target, deadline)?;
                                Ok((captured.partial, captured.reference))
                            }),
                        )
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or(Err(Status::Timeout));
                        match validated {
                            Ok((partial, data)) => {
                                status = if partial {
                                    Status::Partial
                                } else {
                                    Status::Available
                                };
                                reference = Some(data);
                            }
                            Err(reason) => status = reason,
                        }
                    }
                }
                Ok(Ok(Err(reason))) => status = reason,
                _ => {}
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (BUDGET, &mut status, &mut reference);
    let metadata = Metadata {
        status,
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        before_characters: reference
            .as_ref()
            .map_or(0, |r| r.before_cursor.chars().count()),
        selection_characters: reference
            .as_ref()
            .map_or(0, |r| r.selection.chars().count()),
    };
    let mut output =
        serde_json::to_value(metadata).map_err(|_| "Cannot encode context metadata")?;
    if args.include_text {
        if let Some(reference) = reference {
            output["untrusted_reference"] = serde_json::json!({"before_cursor": reference.before_cursor, "selection": reference.selection});
        }
    }
    if args.json || args.include_text {
        println!(
            "{}",
            serde_json::to_string(&output).map_err(|_| "Cannot encode context output")?
        );
    } else {
        println!("{}", output["status"].as_str().unwrap_or("unsupported"));
    }
    Ok(())
}
