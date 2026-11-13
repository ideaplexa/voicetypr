//! Stop a CLI as soon as its result is complete.
//!
//! Claude Code prints its JSON result and then spends about half a second
//! shutting down (measured on 2.1.322: result at 2.08 s, exit at 2.60 s).
//! Polish has no use for that tail, so the cold path returns at the result.
use super::*;

/// The completeness check for a parser, or `None` to wait for exit as before.
pub(super) fn complete_for(output: OutputParser) -> Option<fn(&[u8]) -> bool> {
    match output {
        OutputParser::ClaudeJson => Some(claude_result_complete),
        // These CLIs exit as soon as they finish, or stream lines whose end
        // only the exit marks.
        OutputParser::CodexJsonl | OutputParser::PiJsonl | OutputParser::PlainText => None,
    }
}

/// True once stdout holds a whole, successful Claude result. An error result
/// or a partial document is left to the normal exit path.
fn claude_result_complete(stdout: &[u8]) -> bool {
    parse_claude_json(stdout).is_ok()
}

/// [`drain_bounded`], plus one signal when `complete` first accepts the
/// retained bytes. It keeps draining to EOF so the child never blocks on a
/// full pipe; a truncated capture is never reported as complete.
pub(super) async fn drain_bounded_signalling<R>(
    mut reader: R,
    max_capture_bytes: usize,
    complete: fn(&[u8]) -> bool,
    ready: std::sync::Arc<tokio::sync::Notify>,
) -> std::io::Result<(Vec<u8>, bool)>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut retained = Vec::with_capacity(max_capture_bytes.min(8192));
    let mut chunk = [0_u8; 8192];
    let mut truncated = false;
    let mut signalled = false;
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let take = (max_capture_bytes - retained.len()).min(read);
        retained.extend_from_slice(&chunk[..take]);
        if read > take {
            truncated = true;
        }
        if !signalled && !truncated && complete(&retained) {
            signalled = true;
            ready.notify_one();
        }
    }
    Ok((retained, truncated))
}
