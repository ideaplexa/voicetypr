use std::env;
use std::error::Error;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use serde_json::json;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::Manager;

use crate::audio::recorder::AudioRecorder;
use crate::commands::ai::{ai_provider_key_names, cache_ai_api_key, CacheApiKeyArgs};
use crate::commands::audio::transcribe_audio_file_for_cli;
use crate::commands::keyring::keyring_get;
use crate::commands::license::check_license_status;
use crate::commands::model::get_model_status;
use crate::commands::remote::load_remote_settings;
use crate::commands::settings::get_settings;
use crate::parakeet::ParakeetManager;
use crate::remote::client::{
    self, RemoteServerConnection, TranscriptionRequest, TranscriptionSource,
};
use crate::remote::lifecycle::RemoteServerManager;
use crate::state::AppState;
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSource as ContractSource,
};
use crate::whisper::cache::TranscriberCache;
use crate::whisper::gpu_sidecar::GpuSidecarClient;
use crate::whisper::manager::WhisperManager;

#[derive(Parser, Debug)]
#[command(name = "voicetypr", version, about = "Voicetypr CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Show version, the selected model/engine, language, and which engines are available.
    Status(StatusArgs),
    /// List speech models and whether each one is downloaded.
    Models(OutputArgs),
    /// Transcribe an audio file locally, or via a remote Voicetypr server with --server.
    Transcribe(TranscribeArgs),
    /// Record from the microphone until silence, then transcribe the result.
    Record(RecordArgs),
}

impl CliCommand {
    /// Whether the invoked subcommand requested JSON output, so failures can be emitted
    /// as a parseable error object too.
    fn wants_json(&self) -> bool {
        match self {
            CliCommand::Status(a) => a.json,
            CliCommand::Models(a) => a.json,
            CliCommand::Transcribe(a) => a.json,
            CliCommand::Record(a) => a.json,
        }
    }
}

#[derive(Args, Debug, Clone)]
struct OutputArgs {
    /// Emit JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct StatusArgs {
    /// Emit JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct TranscribeArgs {
    /// Path to the audio file to transcribe (wav, mp3, m4a, flac, ...).
    #[arg(long)]
    file: PathBuf,
    /// Model to use (e.g. "base", "large-v3-turbo"); defaults to the app's selected model. Run `voicetypr models` to list installed models.
    #[arg(long)]
    model: Option<String>,
    /// Engine override ("whisper" or "parakeet"); defaults to the selected model's engine.
    #[arg(long)]
    engine: Option<String>,
    /// Transcribe via a remote Voicetypr server given as host:port (e.g. 192.168.1.10:47842) instead of locally.
    #[arg(long)]
    server: Option<String>,
    /// Remote server password. Prefer --password-stdin or VOICETYPR_REMOTE_PASSWORD for automation.
    #[arg(long)]
    password: Option<String>,
    /// Read the remote server password from stdin (safer than --password for automation).
    #[arg(long)]
    password_stdin: bool,
    /// Emit a JSON result (text, words, metadata); default prints just the transcript text.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct RecordArgs {
    /// Stop recording automatically after a stretch of silence (currently required).
    #[arg(long)]
    until_silence: bool,
    /// Model to use (e.g. "base", "large-v3-turbo"); defaults to the app's selected model. Run `voicetypr models` to list installed models.
    #[arg(long)]
    model: Option<String>,
    /// Engine override ("whisper" or "parakeet"); defaults to the selected model's engine.
    #[arg(long)]
    engine: Option<String>,
    /// Transcribe via a remote Voicetypr server given as host:port (e.g. 192.168.1.10:47842) instead of locally.
    #[arg(long)]
    server: Option<String>,
    /// Remote server password. Prefer --password-stdin or VOICETYPR_REMOTE_PASSWORD for automation.
    #[arg(long)]
    password: Option<String>,
    /// Read the remote server password from stdin (safer than --password for automation).
    #[arg(long)]
    password_stdin: bool,
    /// Emit a JSON result (text, words, metadata, stop_reason); default prints just the transcript text.
    #[arg(long)]
    json: bool,
}

pub fn maybe_run_from_env_with_context(
    context: tauri::Context<tauri::Wry>,
) -> Result<bool, Box<dyn Error>> {
    let first_arg = env::args().nth(1);
    let Some(first_arg) = first_arg.as_deref() else {
        return Ok(false);
    };
    let is_help_or_version = matches!(first_arg, "-h" | "--help" | "help" | "-V" | "--version");
    if !is_help_or_version && !matches!(first_arg, "status" | "models" | "transcribe" | "record") {
        return Ok(false);
    }

    // Windows release builds are GUI-subsystem (see main.rs `windows_subsystem = "windows"`),
    // so the process starts with no console and CLI output would vanish into a null handle.
    // We've confirmed this is a CLI route, so attach the parent terminal's console before
    // any println!/eprintln! or clap --help/--version output is produced.
    #[cfg(windows)]
    attach_parent_console();

    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            use clap::error::ErrorKind;
            // Help/version aren't failures: let clap print them to stdout and exit 0.
            if matches!(
                e.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ErrorKind::DisplayVersion
            ) {
                e.exit();
            }
            // A real usage error: in --json mode emit a parseable object so agents can read
            // why parsing failed; otherwise use clap's human-readable usage output.
            if env::args().skip(1).any(|a| a == "--json") {
                eprintln!("{}", format_cli_error(&e.to_string(), true));
                std::process::exit(2);
            }
            e.exit();
        }
    };
    let Some(command) = cli.command else {
        return Ok(false);
    };

    let wants_json = command.wants_json();
    let result = tauri::async_runtime::block_on(async move {
        let app = build_cli_app(context).await?;
        let app_handle = app.handle().clone();
        warm_ai_key_cache(&app_handle).await?;
        let _ = check_license_status(app_handle.clone()).await;

        match command {
            CliCommand::Status(args) => run_status(&app_handle, args).await?,
            CliCommand::Models(args) => run_models(&app_handle, args).await?,
            CliCommand::Transcribe(args) => run_transcribe(&app_handle, args).await?,
            CliCommand::Record(args) => run_record(&app_handle, args).await?,
        }

        Ok::<(), Box<dyn Error>>(())
    });

    // Format failures for the human or the agent: --json gets a parseable {"error": ...}
    // object on stderr, everyone else gets a plain "Error: ..." line. Either way exit 1 so
    // scripts and agents can branch on the status code.
    if let Err(e) = result {
        emit_cli_error(&*e, wants_json);
        std::process::exit(1);
    }

    Ok(true)
}

/// Print a CLI error to stderr, honoring the subcommand's --json flag.
fn emit_cli_error(err: &dyn Error, json: bool) {
    eprintln!("{}", format_cli_error(&err.to_string(), json));
}

/// Format a CLI error message: a parseable `{"error": "..."}` object in --json mode (so
/// agents can read the failure), or a plain `Error: ...` line otherwise.
fn format_cli_error(message: &str, json: bool) -> String {
    if json {
        serde_json::to_string(&json!({ "error": message }))
            .unwrap_or_else(|_| format!("{{\"error\":\"{}\"}}", message.replace('"', "'")))
    } else {
        format!("Error: {message}")
    }
}

/// Attach this (GUI-subsystem) process to the parent terminal's console so CLI output is
/// visible on Windows. No-op when there is no parent console (e.g. a double-click launch).
///
/// Only streams the shell left unset are pointed at the console, so any redirection is
/// preserved: `voicetypr ... > out.json`, `... 2> err.txt`, and
/// `echo pw | voicetypr --password-stdin` keep their file/pipe handles, and stdin is never
/// overridden. MUST run before the first println!/eprintln!: Rust resolves the std handles
/// lazily on first use, so re-pointing them afterwards would be too late.
#[cfg(windows)]
fn attach_parent_console() {
    use std::fs::OpenOptions;
    use std::mem;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_HANDLE, STD_OUTPUT_HANDLE,
    };

    // SAFETY: kernel32 FFI; ATTACH_PARENT_PROCESS is the documented sentinel process id.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
            return;
        }
    }

    // A stream needs the console screen buffer only when the shell did not already wire it to
    // a file/pipe; GetStdHandle returns NULL for an unset stream on a GUI-subsystem process.
    let needs_console = |which: STD_HANDLE| -> bool {
        // SAFETY: kernel32 FFI.
        match unsafe { GetStdHandle(which) } {
            Ok(h) => h.0.is_null() || h == INVALID_HANDLE_VALUE,
            Err(_) => true,
        }
    };

    let out_unset = needs_console(STD_OUTPUT_HANDLE);
    let err_unset = needs_console(STD_ERROR_HANDLE);
    if !out_unset && !err_unset {
        return;
    }

    // Point only the unset write streams at the console screen buffer; redirected streams keep
    // their inherited handle. We leak the File (mem::forget) because SetStdHandle does not
    // duplicate the handle and we need it for the process lifetime.
    if let Ok(file) = OpenOptions::new().write(true).open("CONOUT$") {
        let handle = HANDLE(file.as_raw_handle());
        // SAFETY: valid console handle held for the process lifetime via mem::forget.
        unsafe {
            if out_unset {
                let _ = SetStdHandle(STD_OUTPUT_HANDLE, handle);
            }
            if err_unset {
                let _ = SetStdHandle(STD_ERROR_HANDLE, handle);
            }
        }
        mem::forget(file);
    }
}

async fn build_cli_app(
    context: tauri::Context<tauri::Wry>,
) -> Result<tauri::App<tauri::Wry>, Box<dyn Error>> {
    #[cfg(not(target_os = "windows"))]
    crate::secure_store::initialize_encryption_key()?;

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .build(context)?;

    #[cfg(target_os = "windows")]
    crate::secure_store::windows_identity::initialize(&app.path().app_data_dir()?)?;

    let models_dir = app.path().app_data_dir()?.join("models");
    std::fs::create_dir_all(&models_dir)?;
    let parakeet_dir = models_dir.join("parakeet");
    std::fs::create_dir_all(&parakeet_dir)?;

    app.manage(AsyncRwLock::new(WhisperManager::new(models_dir.clone())));
    app.manage(ParakeetManager::new(parakeet_dir));
    app.manage(AsyncMutex::new(TranscriberCache::new()));
    app.manage(GpuSidecarClient::new());
    app.manage(AppState::new());
    app.manage(AsyncMutex::new(RemoteServerManager::new()));
    app.manage(AsyncMutex::new(load_remote_settings(app.handle())));

    Ok(app)
}

async fn warm_ai_key_cache(app: &tauri::AppHandle) -> Result<(), Box<dyn Error>> {
    for key_name in ai_provider_key_names() {
        if let Some(api_key) = keyring_get(app.clone(), key_name.clone())? {
            // key_name is "ai_api_key_{provider}"; recover the provider id.
            let provider = key_name
                .strip_prefix("ai_api_key_")
                .map(str::to_string)
                .unwrap_or(key_name.clone());
            cache_ai_api_key(app.clone(), CacheApiKeyArgs { provider, api_key }).await?;
        }
    }
    Ok(())
}

async fn run_status(app: &tauri::AppHandle, args: StatusArgs) -> Result<(), Box<dyn Error>> {
    let settings = get_settings(app.clone()).await?;
    let availability = crate::recognition_availability_snapshot(app).await;

    if args.json {
        let payload = json!({
            "version": env!("CARGO_PKG_VERSION"),
            "settings": {
                "current_model": settings.current_model,
                "current_model_engine": settings.current_model_engine,
                "speech_language": settings.speech_language,
                "transcription_task": settings.transcription_task,
                "final_text_language": settings.final_text_language,
            },
            "availability": availability,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("version:  {}", env!("CARGO_PKG_VERSION"));
        println!(
            "model:    {} ({})",
            settings.current_model, settings.current_model_engine
        );
        println!("language: {}", settings.speech_language);
        println!("engines:  {}", format_availability(&availability));
    }
    Ok(())
}

async fn run_models(app: &tauri::AppHandle, args: OutputArgs) -> Result<(), Box<dyn Error>> {
    let response = get_model_status(
        app.state::<AsyncRwLock<WhisperManager>>(),
        app.state::<ParakeetManager>(),
        app.clone(),
    )
    .await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        for m in &response.models {
            let state = if m.downloaded {
                "ready"
            } else {
                "not downloaded"
            };
            println!("{:40} {:10} {}", m.display_name, m.engine, state);
        }
    }
    Ok(())
}

async fn run_transcribe(
    app: &tauri::AppHandle,
    args: TranscribeArgs,
) -> Result<(), Box<dyn Error>> {
    if !args.file.exists() {
        return Err(format!("Audio file not found: {}", args.file.display()).into());
    }
    if args.server.is_none() {
        check_license_status(app.clone()).await?;
    }
    let payload = if let Some(server) = args.server.as_deref() {
        let password = resolve_password(args.password.clone(), args.password_stdin)?;
        transcribe_via_remote(app, &args.file, server, password).await?
    } else {
        let settings = get_settings(app.clone()).await?;
        let model = args
            .model
            .clone()
            .or_else(|| {
                (!settings.current_model.is_empty()).then_some(settings.current_model.clone())
            })
            .ok_or_else(|| "No model specified and no model is selected in the app. Pass --model <name> (run `voicetypr models` to list installed models) or choose a default model in Voicetypr settings.".to_string())?;
        let engine = args.engine.clone().or_else(|| {
            (!settings.current_model_engine.is_empty())
                .then_some(settings.current_model_engine.clone())
        });
        let t = transcribe_audio_file_for_cli(
            app.clone(),
            args.file.to_string_lossy().to_string(),
            model.clone(),
            engine.clone(),
        )
        .await?;
        json!({ "text": t.text, "words": t.words, "metadata": t.metadata, "model": model, "engine": engine })
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", payload["text"].as_str().unwrap_or_default());
    }
    Ok(())
}

async fn run_record(app: &tauri::AppHandle, args: RecordArgs) -> Result<(), Box<dyn Error>> {
    if !args.until_silence {
        return Err(
            "`record` currently supports only stop-on-silence. Re-run with --until-silence.".into(),
        );
    }

    check_license_status(app.clone()).await?;
    let settings = get_settings(app.clone()).await?;
    let recordings_dir = app.path().app_data_dir()?.join("recordings");
    std::fs::create_dir_all(&recordings_dir)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S%3f");
    let uuid_part = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let output_path = recordings_dir.join(format!("cli-recording-{}_{}.wav", timestamp, uuid_part));
    let mut recording_guard = TempRecording {
        path: output_path.clone(),
        delete_on_drop: false,
    };

    let mut recorder = AudioRecorder::new();
    recorder.start_recording(
        output_path
            .to_str()
            .ok_or_else(|| "Invalid recording path".to_string())?,
        settings.selected_microphone.clone(),
    )?;
    eprintln!("Recording… stop by silence.");
    let stop_message = recorder.wait_for_recording_end()?;

    let payload = if let Some(server) = args.server.as_deref() {
        let password = resolve_password(args.password.clone(), args.password_stdin)?;
        let mut payload = transcribe_via_remote(app, &output_path, server, password).await?;
        payload["stop_reason"] = json!(stop_message);
        payload
    } else {
        let model = args
            .model
            .clone()
            .or_else(|| {
                (!settings.current_model.is_empty()).then_some(settings.current_model.clone())
            })
            .ok_or_else(|| "No model specified and no model is selected in the app. Pass --model <name> (run `voicetypr models` to list installed models) or choose a default model in Voicetypr settings.".to_string())?;
        let engine = args.engine.clone().or_else(|| {
            (!settings.current_model_engine.is_empty())
                .then_some(settings.current_model_engine.clone())
        });
        let t = transcribe_audio_file_for_cli(
            app.clone(),
            output_path.to_string_lossy().to_string(),
            model.clone(),
            engine.clone(),
        )
        .await?;
        json!({ "text": t.text, "words": t.words, "metadata": t.metadata, "model": model, "engine": engine, "stop_reason": stop_message })
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", payload["text"].as_str().unwrap_or_default());
    }
    recording_guard.delete_on_drop = true;
    Ok(())
}

struct RemoteNormalizedAudio {
    path: PathBuf,
}

impl RemoteNormalizedAudio {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RemoteNormalizedAudio {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "Failed to remove CLI normalized temp file {:?}: {}",
                    self.path,
                    error
                );
            }
        }
    }
}

struct TempRecording {
    path: PathBuf,
    /// Only remove the captured WAV after a successful transcription. On the
    /// error path the recording is preserved so a long capture is not lost to a
    /// transient model/network failure and can be retried.
    delete_on_drop: bool,
}

impl Drop for TempRecording {
    fn drop(&mut self) {
        if self.delete_on_drop {
            if let Err(error) = std::fs::remove_file(&self.path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    log::warn!(
                        "Failed to remove CLI temp recording {:?}: {}",
                        self.path,
                        error
                    );
                }
            }
        } else if std::fs::metadata(&self.path).is_ok() {
            // Error path: keep the capture so the user can retry transcription
            // instead of losing a long recording to a transient failure.
            eprintln!(
                "Recording preserved for retry (CLI exited with an error): {}",
                self.path.display()
            );
        }
    }
}

async fn normalize_audio_for_remote(
    app: &tauri::AppHandle,
    file: &Path,
) -> Result<RemoteNormalizedAudio, Box<dyn Error>> {
    let recordings_dir = app.path().app_data_dir()?.join("recordings");
    std::fs::create_dir_all(&recordings_dir)?;

    // Millisecond timestamps alone collide for simultaneous CLI invocations;
    // add a uuid suffix (matches the recordings naming convention in
    // commands/audio.rs) for guaranteed-unique temp paths.
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S%3f");
    let uuid_part = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let output_path = recordings_dir.join(format!(
        "cli-remote-normalized-{}_{}.wav",
        timestamp, uuid_part
    ));
    crate::ffmpeg::normalize_streaming(app, file, &output_path)
        .await
        .map_err(std::io::Error::other)?;

    Ok(RemoteNormalizedAudio { path: output_path })
}

async fn transcribe_via_remote(
    app: &tauri::AppHandle,
    file: &Path,
    server: &str,
    password: Option<String>,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let settings = get_settings(app.clone()).await?;
    let (host, port) = parse_server(server)?;
    let normalized_file = normalize_audio_for_remote(app, file).await?;
    let audio_data = std::fs::read(normalized_file.path())?;
    let request = TranscriptionRequest::new(audio_data, TranscriptionSource::Upload)
        .with_language_and_task(
            Some(settings.speech_language.clone()),
            Some(settings.transcription_task.clone()),
        );
    let timeout_ms = client::timeout_ms_for_wav_file(
        normalized_file.path().to_string_lossy().as_ref(),
        TranscriptionSource::Upload,
    );
    let connection = RemoteServerConnection::new(host, port, password);
    let response = client::transcribe_audio(&connection, request, timeout_ms).await?;

    let job = TranscriptionJob::from_legacy_settings(
        ContractSource::RemoteServer,
        "remote",
        response.model.clone(),
        Some(settings.speech_language.clone()),
        settings.transcription_task
            == crate::commands::settings::TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH,
    );
    let transcription = TranscriptionResult::new(&job, response.text)
        .with_transcript_language(response.transcript_language)
        .with_processing_duration_ms(Some(response.duration_ms));
    let writing = crate::writing::process_transcription(app.clone(), transcription)
        .await
        .map_err(|error| std::io::Error::other(error.user_message()))?;

    Ok(json!({
        "text": writing.final_text,
        "output_language": writing.output_language,
        "mode": writing.mode,
        "applied_operations": writing.applied_operations,
        "warnings": writing.warnings,
        "model": response.model,
        "duration_ms": response.duration_ms,
    }))
}

fn resolve_password(
    password: Option<String>,
    password_stdin: bool,
) -> Result<Option<String>, Box<dyn Error>> {
    if password_stdin {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        let trimmed = line.trim().to_owned();
        return Ok(if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        });
    }
    if password.is_some() {
        return Ok(password);
    }
    Ok(env::var("VOICETYPR_REMOTE_PASSWORD")
        .ok()
        .filter(|s| !s.is_empty()))
}

fn parse_server(value: &str) -> Result<(String, u16), Box<dyn Error>> {
    let (host, port_str) = value.rsplit_once(':').ok_or_else(|| {
        format!("Invalid --server '{value}': expected host:port, e.g. 192.168.1.10:47842")
    })?;
    if host.is_empty() {
        return Err(format!("Invalid --server '{value}': missing host, expected host:port").into());
    }
    let port = port_str.parse::<u16>().map_err(|_| {
        format!("Invalid --server '{value}': '{port_str}' is not a valid port (1-65535)")
    })?;
    if port == 0 {
        return Err(
            format!("Invalid --server '{value}': '0' is not a valid port (1-65535)").into(),
        );
    }
    Ok((host.to_string(), port))
}

fn format_availability(snap: &crate::RecognitionAvailabilitySnapshot) -> String {
    let parts: Vec<&str> = [
        snap.whisper_available.then_some("whisper"),
        snap.parakeet_available.then_some("parakeet"),
        (snap.cloud_selected && snap.cloud_ready).then_some("cloud"),
        snap.remote_available.then_some("remote"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_server_requires_port() {
        assert!(parse_server("localhost").is_err());
        let parsed = parse_server("localhost:47842").unwrap();
        assert_eq!(parsed.0, "localhost");
        assert_eq!(parsed.1, 47842);
    }

    #[test]
    fn cli_parses_transcribe_command() {
        let cli = Cli::try_parse_from([
            "voicetypr",
            "transcribe",
            "--file",
            "sample.wav",
            "--server",
            "localhost:47842",
            "--password",
            "secret",
            "--json",
        ])
        .unwrap();

        match cli.command {
            Some(CliCommand::Transcribe(args)) => {
                assert_eq!(args.file, PathBuf::from("sample.wav"));
                assert_eq!(args.server.as_deref(), Some("localhost:47842"));
                assert_eq!(args.password.as_deref(), Some("secret"));
                assert!(args.json);
            }
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_record_command() {
        let cli =
            Cli::try_parse_from(["voicetypr", "record", "--until-silence", "--json"]).unwrap();

        match cli.command {
            Some(CliCommand::Record(args)) => {
                assert!(args.until_silence);
                assert!(args.json);
            }
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_status_json_flag() {
        let cli = Cli::try_parse_from(["voicetypr", "status", "--json"]).unwrap();
        match cli.command {
            Some(CliCommand::Status(args)) => assert!(args.json),
            other => panic!("unexpected command: {:?}", other),
        }

        let cli = Cli::try_parse_from(["voicetypr", "status"]).unwrap();
        match cli.command {
            Some(CliCommand::Status(args)) => assert!(!args.json),
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_models_json_flag() {
        let cli = Cli::try_parse_from(["voicetypr", "models", "--json"]).unwrap();
        match cli.command {
            Some(CliCommand::Models(args)) => assert!(args.json),
            other => panic!("unexpected command: {:?}", other),
        }

        let cli = Cli::try_parse_from(["voicetypr", "models"]).unwrap();
        match cli.command {
            Some(CliCommand::Models(args)) => assert!(!args.json),
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn format_availability_none_when_all_false() {
        let snap = crate::RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            cloud_selected: false,
            cloud_ready: false,
            remote_selected: false,
            remote_status: crate::remote::settings::ConnectionStatus::default(),
            remote_last_checked: 0,
            remote_available: false,
        };
        assert_eq!(format_availability(&snap), "none");
    }

    #[test]
    fn format_availability_lists_available_engines() {
        let snap = crate::RecognitionAvailabilitySnapshot {
            whisper_available: true,
            parakeet_available: false,
            cloud_selected: true,
            cloud_ready: true,
            remote_selected: false,
            remote_status: crate::remote::settings::ConnectionStatus::default(),
            remote_last_checked: 0,
            remote_available: true,
        };
        assert_eq!(format_availability(&snap), "whisper, cloud, remote");
    }

    #[test]
    fn format_availability_cloud_requires_ready() {
        let snap = crate::RecognitionAvailabilitySnapshot {
            whisper_available: false,
            parakeet_available: false,
            cloud_selected: true,
            cloud_ready: false, // selected but not ready
            remote_selected: false,
            remote_status: crate::remote::settings::ConnectionStatus::default(),
            remote_last_checked: 0,
            remote_available: false,
        };
        assert_eq!(format_availability(&snap), "none");
    }

    #[test]
    fn format_cli_error_json_is_parseable() {
        let s = format_cli_error("boom \"quoted\"", true);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["error"], "boom \"quoted\"");
    }

    #[test]
    fn format_cli_error_plain_is_prefixed() {
        assert_eq!(format_cli_error("boom", false), "Error: boom");
    }

    #[test]
    fn parse_server_rejects_nonnumeric_port() {
        let err = parse_server("host:abc").unwrap_err().to_string();
        assert!(err.contains("not a valid port"), "got: {err}");
    }

    #[test]
    fn parse_server_rejects_missing_host() {
        let err = parse_server(":47842").unwrap_err().to_string();
        assert!(err.contains("missing host"), "got: {err}");
    }

    #[test]
    fn parse_server_rejects_port_zero() {
        let err = parse_server("host:0").unwrap_err().to_string();
        assert!(err.contains("not a valid port"), "got: {err}");
    }

    #[test]
    fn wants_json_reflects_flag() {
        let cli = Cli::try_parse_from(["voicetypr", "status", "--json"]).unwrap();
        assert!(cli.command.unwrap().wants_json());
        let cli = Cli::try_parse_from(["voicetypr", "status"]).unwrap();
        assert!(!cli.command.unwrap().wants_json());
    }

    #[test]
    fn warm_ai_key_cache_uses_catalog_provider_list() {
        // warm_ai_key_cache now iterates ai_provider_key_names() instead of a
        // hardcoded array. The dynamic list must still cover every provider the
        // hardcoded list did, and yield exact ai_api_key_<provider> keys.
        let names = ai_provider_key_names();
        for legacy in ["openai", "anthropic", "gemini", "custom"] {
            let key = format!("ai_api_key_{}", legacy);
            assert!(names.contains(&key), "dynamic key list missing {}", key);
        }
        // Every key is a proper ai_api_key_<provider>; stripping recovers a
        // valid provider id (what CacheApiKeyArgs re-prefixes).
        for key_name in &names {
            assert!(
                key_name.starts_with("ai_api_key_"),
                "unexpected key shape: {}",
                key_name
            );
        }
    }
}
