//! Serialized, cancellable JSON IPC. The owner discards a process after any
//! request failure, so partial writes or late replies cannot poison a new take.
use super::messages::{Command, Response};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tauri::Manager;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};

pub struct SidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    pub gpu: bool,
    reusable: bool,
}

// Task abort drops futures instead of returning an error. Kill synchronously on
// that path and poison the cached process so its unread reply is never reused.
struct PendingRequest<'a> {
    child: &'a mut Child,
    reusable: &'a mut bool,
}
impl Drop for PendingRequest<'_> {
    fn drop(&mut self) {
        if !*self.reusable {
            let _ = self.child.start_kill();
        }
    }
}

impl SidecarProcess {
    pub fn is_reusable(&self) -> bool {
        self.reusable
    }
    pub fn spawn(executable: &Path, gpu: bool) -> Result<Self, String> {
        Self::start(tokio::process::Command::new(executable), gpu)
    }
    fn start(mut command: tokio::process::Command, gpu: bool) -> Result<Self, String> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(target_os = "windows")]
        command.creation_flags(0x08000000);
        let mut child = command
            .spawn()
            .map_err(|_| "CrispASR runtime could not start. Repair or reinstall the app.")?;
        let stdin = child.stdin.take().ok_or("CrispASR input unavailable")?;
        let stdout =
            BufReader::new(child.stdout.take().ok_or("CrispASR output unavailable")?).lines();
        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: 0,
            gpu,
            reusable: true,
        })
    }

    pub async fn request(
        &mut self,
        command: Command<'_>,
        timeout: Duration,
        cancelled: &(impl Fn() -> bool + Sync),
        on_partial: &mut (impl FnMut(u64, String, String) + Send),
    ) -> Result<Response, String> {
        if !self.reusable {
            return Err("CrispASR request was interrupted".into());
        }
        self.next_id += 1;
        let id = self.next_id;
        let mut payload = serde_json::to_value(command).map_err(|_| "Invalid CrispASR request")?;
        payload["id"] = serde_json::json!(id);
        let mut line = serde_json::to_vec(&payload).map_err(|_| "Invalid CrispASR request")?;
        line.push(b'\n');
        self.reusable = false;
        let pending = PendingRequest {
            child: &mut self.child,
            reusable: &mut self.reusable,
        };
        let operation = async {
            self.stdin
                .write_all(&line)
                .await
                .map_err(|_| "CrispASR runtime disconnected")?;
            self.stdin
                .flush()
                .await
                .map_err(|_| "CrispASR runtime disconnected")?;
            loop {
                let line = self
                    .stdout
                    .next_line()
                    .await
                    .map_err(|_| "CrispASR runtime disconnected")?
                    .ok_or("CrispASR runtime exited")?;
                let response: Response =
                    serde_json::from_str(&line).map_err(|_| "Invalid CrispASR response")?;
                if response.id() != id {
                    return Err("CrispASR response ID mismatch".to_string());
                }
                match response {
                    Response::Partial { session_id, committed, tentative, .. } => on_partial(session_id, committed, tentative),
                    Response::Error { code, .. } => return Err(match code.as_str() {
                        "model_not_found" => "CrispASR model is missing. Download it again.",
                        "model_load_failed" => "CrispASR could not load the model. Try CPU acceleration or repair the model.",
                        "stream_limit" => "CrispASR stream limit reached",
                        _ => "CrispASR transcription failed",
                    }.to_string()),
                    response => return Ok(response),
                }
            }
        };
        let result = tokio::select! {
            result = tokio::time::timeout(timeout, operation) => result.unwrap_or_else(|_| Err("Transcription timed out".into())),
            _ = async { while !cancelled() { tokio::time::sleep(Duration::from_millis(20)).await; } } => Err("Transcription cancelled".into()),
        };
        *pending.reusable = result.is_ok();
        drop(pending);
        if result.is_err() {
            self.abort().await;
        }
        result
    }
    pub async fn abort(&mut self) {
        let _ = self.child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
    }
}

pub fn runtime_path(app: &tauri::AppHandle, gpu: bool) -> Option<PathBuf> {
    if gpu && cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        return None;
    }
    let variant = if gpu { "gpu" } else { "cpu" };
    let extension = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    let triple = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        _ => return None,
    };
    let names = [
        format!("crispasr-sidecar-{variant}{extension}"),
        format!("crispasr-sidecar-{variant}-{triple}{extension}"),
    ];
    let roots = [
        app.path().resource_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf)),
        Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sidecar/crispasr/dist")),
    ];
    roots
        .into_iter()
        .flatten()
        .flat_map(|root| names.iter().map(move |name| root.join(name)))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn fixture(mode: &str) -> SidecarProcess {
        let mut command = tokio::process::Command::new("node");
        command
            .arg(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("src/crispasr/tests/sidecar-fixture.mjs"),
            )
            .arg(mode);
        SidecarProcess::start(command, false).unwrap()
    }

    #[tokio::test]
    #[ignore = "requires CRISPASR_TEST_BINARY, MODEL, BACKEND and AUDIO real-speech fixtures"]
    async fn real_model_protocol_round_trip() {
        let executable = std::env::var("CRISPASR_TEST_BINARY").expect("native sidecar fixture");
        let model = std::env::var("CRISPASR_TEST_MODEL").expect("model fixture");
        let backend = std::env::var("CRISPASR_TEST_BACKEND").expect("backend fixture");
        let audio = std::env::var("CRISPASR_TEST_AUDIO").expect("real-speech fixture");
        let gpu = std::env::var("CRISPASR_TEST_GPU").as_deref() == Ok("1");
        let samples =
            crate::crispasr::pcm::read_wav(Path::new(&audio), &AtomicBool::new(false)).unwrap();
        let mut process = SidecarProcess::spawn(Path::new(&executable), gpu).unwrap();
        assert!(matches!(
            process
                .request(
                    Command::Status,
                    Duration::from_secs(10),
                    &|| false,
                    &mut |_, _, _| {}
                )
                .await
                .unwrap(),
            Response::Status { protocol: 1, .. }
        ));
        process
            .request(
                Command::LoadModel {
                    model: "fixture",
                    backend: &backend,
                    model_path: &model,
                    gpu,
                    threads: 4,
                },
                Duration::from_secs(60),
                &|| false,
                &mut |_, _, _| {},
            )
            .await
            .unwrap();
        for (session_id, mode) in [(51, "batch"), (52, "recording")] {
            let mut partials = 0;
            let mut committed = String::new();
            let mut partial = |id, next: String, tentative: String| {
                assert_eq!(id, session_id);
                assert!(next.starts_with(&committed));
                if !next.is_empty() || !tentative.is_empty() {
                    partials += 1;
                }
                committed = next;
            };
            process
                .request(
                    Command::StartStream {
                        session_id,
                        language: "auto",
                        mode,
                    },
                    Duration::from_secs(10),
                    &|| false,
                    &mut partial,
                )
                .await
                .unwrap();
            for chunk in samples.chunks(16_000) {
                let pcm = super::super::messages::encode_audio(chunk);
                process
                    .request(
                        Command::AudioChunk {
                            session_id,
                            pcm: &pcm,
                        },
                        Duration::from_secs(60),
                        &|| false,
                        &mut partial,
                    )
                    .await
                    .unwrap();
            }
            let result = process
                .request(
                    Command::FinalizeStream { session_id },
                    Duration::from_secs(180),
                    &|| false,
                    &mut partial,
                )
                .await
                .unwrap();
            assert!(
                matches!(result, Response::Final { session_id: id, result, .. }
                if id == session_id && result.samples == samples.len() as u64 && !result.text.is_empty())
            );
            if mode == "recording" {
                assert!(partials > 0);
            }
        }
        process.abort().await;
    }

    #[tokio::test]
    async fn dropping_an_in_flight_request_terminates_native_work() {
        let mut process = fixture("stall");
        {
            let mut partial = |_, _, _| {};
            let request = process.request(
                Command::Status,
                Duration::from_secs(60),
                &|| false,
                &mut partial,
            );
            tokio::pin!(request);
            assert!(matches!(
                futures_util::poll!(request.as_mut()),
                std::task::Poll::Pending
            ));
            // A JoinHandle abort drops this future without reaching request's
            // ordinary Result cleanup, just like cancel_recording does.
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(1), process.child.wait())
                .await
                .is_ok()
        );
        assert!(!process.is_reusable());
    }

    #[tokio::test]
    async fn mismatched_reply_kills_the_process_instead_of_reusing_it() {
        let mut process = fixture("wrong_id");
        let result = process
            .request(
                Command::Status,
                Duration::from_secs(5),
                &|| false,
                &mut |_, _, _| {},
            )
            .await;
        assert!(matches!(result, Err(error) if error.contains("ID mismatch")));
        assert!(process.child.try_wait().unwrap().is_some());
    }

    #[tokio::test]
    async fn stalled_native_request_is_cancellable() {
        let mut process = fixture("stall");
        let flag = AtomicBool::new(false);
        let cancelled = || flag.load(Ordering::SeqCst);
        let mut partial = |_, _, _| {};
        {
            let request = process.request(
                Command::Status,
                Duration::from_secs(5),
                &cancelled,
                &mut partial,
            );
            tokio::pin!(request);
            assert!(matches!(
                futures_util::poll!(request.as_mut()),
                std::task::Poll::Pending
            ));
            flag.store(true, Ordering::SeqCst);
            assert!(matches!(request.await, Err(error) if error.contains("cancelled")));
        }
        assert!(process.child.try_wait().unwrap().is_some());
    }

    #[tokio::test]
    async fn stalled_native_request_has_a_hard_deadline() {
        let mut process = fixture("stall");
        let result = process
            .request(
                Command::Status,
                Duration::from_millis(30),
                &|| false,
                &mut |_, _, _| {},
            )
            .await;
        assert!(matches!(result, Err(error) if error.contains("timed out")));
        assert!(process.child.try_wait().unwrap().is_some());
    }
}
