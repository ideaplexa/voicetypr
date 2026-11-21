//! A private FIFO carries the system prompt, never a regular disk file or argv
//! value. pi reloads this resource on new_session, so each reset feeds it anew.
#[cfg(target_os = "macos")]
use super::*;
#[cfg(target_os = "macos")]
use std::{io::Write, os::unix::fs::OpenOptionsExt, sync::Arc};

#[cfg(target_os = "macos")]
pub(super) struct MemoryPrompt {
    directory: TempDir,
    prompt: String,
    stopped: Arc<std::sync::atomic::AtomicBool>,
}
#[cfg(target_os = "macos")]
impl MemoryPrompt {
    pub(super) fn attach(command: &mut Command, prompt: &str, flag: &str) -> Result<Self, MappedAiProviderError> {
        use std::os::unix::ffi::OsStrExt;
        if prompt.len() > MAX_CAPTURE_BYTES { return Err(failure()); }
        let directory = private_input::private_directory()?;
        let path = directory.path().join("prompt.pipe");
        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| failure())?;
        // SAFETY: a valid NUL-terminated path in our owner-only directory.
        if unsafe { libc::mkfifo(name.as_ptr(), 0o600) } != 0 { return Err(failure()); }
        command.arg(flag).arg(&path);
        Ok(Self { directory, prompt: prompt.to_owned(), stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)) })
    }

    pub(super) fn feed(&self) -> tokio::task::JoinHandle<()> {
        let path = self.directory.path().join("prompt.pipe");
        let prompt = self.prompt.clone();
        let stopped = self.stopped.clone();
        tokio::task::spawn_blocking(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let active = || !stopped.load(Ordering::SeqCst) && Instant::now() < deadline;
            let mut writer = loop {
                if !active() { return; }
                match std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW).open(&path) {
                    Ok(writer) => break writer,
                    Err(error) if error.raw_os_error() == Some(libc::ENXIO) => std::thread::sleep(Duration::from_millis(5)),
                    Err(_) => return,
                }
            };
            let mut bytes = prompt.as_bytes();
            while !bytes.is_empty() && active() {
                match writer.write(bytes) {
                    Ok(0) => return,
                    Ok(count) => bytes = &bytes[count..],
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(5)),
                    Err(_) => return,
                }
            }
            // Closing the sole writer supplies EOF to readFileSync. Never start
            // another writer until the corresponding RPC response has arrived.
        })
    }
}
#[cfg(target_os = "macos")]
impl Drop for MemoryPrompt {
    fn drop(&mut self) { self.stopped.store(true, Ordering::SeqCst); }
}
#[cfg(target_os = "macos")]
fn failure() -> MappedAiProviderError { MappedAiProviderError::new(AiProviderError::BadResponse) }

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::FileTypeExt;

    #[tokio::test]
    async fn reset_reloads_prompt_without_persisting_content() {
        let mut command = Command::new("pi");
        let prompt = MemoryPrompt::attach(&mut command, "Correct only this take.", "--system-prompt").unwrap();
        let path = prompt.directory.path().join("prompt.pipe");
        assert!(std::fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
        for _ in 0..2 {
            let feed = prompt.feed();
            let path = path.clone();
            let bytes = tokio::task::spawn_blocking(move || std::fs::read(path)).await.unwrap().unwrap();
            feed.await.unwrap();
            assert_eq!(bytes, b"Correct only this take.");
        }
        assert_eq!(std::fs::read_dir(prompt.directory.path()).unwrap().count(), 1);
        drop(prompt);
        assert!(!path.exists());
    }
}
