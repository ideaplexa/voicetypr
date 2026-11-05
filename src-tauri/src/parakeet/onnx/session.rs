//! Windows x64 CPU adapter. The slot owns the only TDT model and blocking worker.
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use parakeet_rs::{ExecutionConfig, ExecutionProvider, ParakeetTDT, TimestampMode, Transcriber};

use super::slot::{Model, Output, Slot, Word};

struct TdtModel(ParakeetTDT);

impl Model for TdtModel {
    fn transcribe_chunk(&mut self, samples: &[f32]) -> Result<Output, String> {
        let result = self
            .0
            .transcribe_samples(samples.to_vec(), 16_000, 1, Some(TimestampMode::Words))
            .map_err(|error| classify_model_error(&error.to_string()))?;
        Ok(Output {
            text: result.text,
            words: result
                .tokens
                .into_iter()
                .map(|token| Word {
                    text: token.text,
                    start: token.start,
                    end: token.end,
                })
                .collect(),
        })
    }
}

fn classify_model_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("alloc")
        || lower.contains("out of memory")
        || lower.contains("not enough memory")
    {
        "Parakeet needs more available memory. Close other apps and try again.".into()
    } else if lower.contains("illegal instruction")
        || lower.contains("unsupported")
        || lower.contains("cpu instruction")
    {
        "This CPU cannot run the Parakeet ONNX model.".into()
    } else if lower.contains("no such file")
        || lower.contains("not found")
        || lower.contains("missing")
    {
        "Parakeet model files are missing. Download the model again.".into()
    } else {
        "Parakeet model files may be corrupt. Delete and download the model again.".into()
    }
}

fn build_model(directory: &Path) -> Result<TdtModel, String> {
    crate::onnx_runtime::require_cpu().map_err(str::to_owned)?;
    for filename in [
        "encoder-model.int8.onnx",
        "decoder_joint-model.int8.onnx",
        "vocab.txt",
    ] {
        if !directory.join(filename).is_file() {
            return Err("Parakeet model files are missing. Download the model again.".into());
        }
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, |count| count.get())
        .min(4);
    let config = ExecutionConfig::new()
        .with_execution_provider(ExecutionProvider::Cpu)
        .with_intra_threads(threads)
        .with_inter_threads(1);
    ParakeetTDT::from_pretrained(directory, Some(config))
        .map(TdtModel)
        .map_err(|error| classify_model_error(&error.to_string()))
}

pub struct OnnxSession {
    slot: Slot<TdtModel>,
}

impl OnnxSession {
    pub fn new() -> Self {
        Self {
            slot: Slot::new(build_model),
        }
    }

    pub fn ensure_not_shutdown(&self) -> Result<(), String> {
        self.slot.ensure_not_shutdown()
    }

    pub async fn load(&self, directory: PathBuf, cancel: Arc<AtomicBool>) -> Result<(), String> {
        self.slot.load(directory, cancel).await
    }

    pub async fn transcribe_wav(
        &self,
        path: PathBuf,
        cancel: Arc<AtomicBool>,
    ) -> Result<(Output, f32), String> {
        self.slot.transcribe_wav(path, cancel).await
    }

    pub async fn unload(&self) {
        self.slot.unload().await;
    }

    pub async fn shutdown(&self) {
        self.slot.shutdown().await;
    }

    /// Call only from a blocking thread, never from an async task.
    pub fn unload_blocking(&self) {
        self.slot.unload_blocking();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unsupported_cpu_never_enters_model_initialization() {
        crate::onnx_runtime::without_cpu(|| {
            let error = super::build_model(std::path::Path::new("unused"))
                .err()
                .expect("unsupported CPU");
            assert_eq!(error, crate::onnx_runtime::CPU_UNSUPPORTED);
        });
    }

    #[test]
    fn builder_reports_missing_files_without_path_or_ort_detail() {
        let missing = std::env::temp_dir().join("voicetypr-onnx-model-does-not-exist");
        let error = super::build_model(&missing).err().expect("missing model");
        if let Err(reason) = crate::onnx_runtime::require_cpu() {
            assert_eq!(error, reason);
            return;
        }
        assert_eq!(
            error,
            "Parakeet model files are missing. Download the model again."
        );
    }

    #[test]
    fn builder_sanitizes_a_real_model_init_failure_without_loading_a_model() {
        let directory = tempfile::tempdir().expect("temp directory");
        std::fs::write(directory.path().join("encoder-model.int8.onnx"), b"invalid")
            .expect("encoder placeholder");
        std::fs::write(
            directory.path().join("decoder_joint-model.int8.onnx"),
            b"invalid",
        )
        .expect("decoder placeholder");
        std::fs::write(directory.path().join("vocab.txt"), b"a\nb\n")
            .expect("vocabulary placeholder");
        let error = super::build_model(directory.path())
            .err()
            .expect("invalid model");
        assert!(!error.contains(&directory.path().display().to_string()));
        assert!(error.starts_with("Parakeet ") || error.starts_with("This CPU"));
    }
}
