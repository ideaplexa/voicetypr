//! Clipboard delivery owns an OS change token, never a text-equality token.
//! macOS uses eager data and a timer fallback; see the receipt feasibility report.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(crate) use macos::{begin, finish, verify_slow_reader};
pub(crate) mod cli;
