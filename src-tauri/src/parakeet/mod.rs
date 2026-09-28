pub mod error;
pub mod manager;
pub mod messages;
pub mod models;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub(crate) mod onnx;
pub mod sidecar;

pub use manager::{ParakeetManager, ParakeetModelStatus};
