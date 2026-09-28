//! Serialized model ownership shared by the Windows adapter and platform-neutral tests.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::Mutex as AsyncMutex;

use super::chunking;

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f32,
    pub end: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Output {
    pub text: String,
    pub words: Vec<Word>,
}

pub trait Model: Send + 'static {
    fn transcribe_chunk(&mut self, samples: &[f32]) -> Result<Output, String>;
}

type Factory<M> = dyn Fn(&Path) -> Result<M, String> + Send + Sync;

struct State<M> {
    directory: Option<PathBuf>,
    model: Option<M>,
}

struct Core<M> {
    state: Mutex<State<M>>,
    current_directory: Mutex<Option<PathBuf>>,
    lifecycle: AsyncMutex<()>,
    generation: AtomicU64,
    unload_epoch: AtomicU64,
    admitting: AtomicBool,
    cancel_worker: AtomicBool,
    factory: Arc<Factory<M>>,
}

pub struct Slot<M> {
    core: Arc<Core<M>>,
}

impl<M: Model> Slot<M> {
    pub fn new(factory: impl Fn(&Path) -> Result<M, String> + Send + Sync + 'static) -> Self {
        Self {
            core: Arc::new(Core {
                state: Mutex::new(State {
                    directory: None,
                    model: None,
                }),
                current_directory: Mutex::new(None),
                lifecycle: AsyncMutex::new(()),
                generation: AtomicU64::new(0),
                unload_epoch: AtomicU64::new(0),
                admitting: AtomicBool::new(false),
                cancel_worker: AtomicBool::new(false),
                factory: Arc::new(factory),
            }),
        }
    }

    pub async fn load(&self, directory: PathBuf, cancel: Arc<AtomicBool>) -> Result<(), String> {
        let unload_epoch = self.core.unload_epoch.load(Ordering::SeqCst);
        let _lifecycle = self.core.lifecycle.lock().await;
        if cancel.load(Ordering::SeqCst)
            || self.core.unload_epoch.load(Ordering::SeqCst) != unload_epoch
        {
            return Err("Transcription cancelled".into());
        }
        // A different load supersedes any active worker. A same-directory load
        // waits behind it and reuses the model.
        let same_directory = self
            .core
            .current_directory
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            == Some(&directory);
        if same_directory && self.core.admitting.load(Ordering::SeqCst) {
            return if cancel.load(Ordering::SeqCst) {
                Err("Transcription cancelled".into())
            } else {
                Ok(())
            };
        }
        if !same_directory {
            self.core.admitting.store(false, Ordering::SeqCst);
            self.core.cancel_worker.store(true, Ordering::SeqCst);
        }
        let generation = self.core.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.core.admitting.store(true, Ordering::SeqCst);
        self.core.cancel_worker.store(false, Ordering::SeqCst);
        let core = Arc::clone(&self.core);
        tokio::task::spawn_blocking(move || {
            let mut state = core.state.lock().unwrap_or_else(|error| error.into_inner());
            if cancel.load(Ordering::SeqCst)
                || !core.admitting.load(Ordering::SeqCst)
                || core.generation.load(Ordering::SeqCst) != generation
            {
                return Err("Transcription cancelled".into());
            }
            if state.directory.as_ref() == Some(&directory) && state.model.is_some() {
                return Ok(());
            }
            state.model = None;
            state.directory = None;
            *core
                .current_directory
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = None;
            let model = (core.factory)(&directory)?;
            if cancel.load(Ordering::SeqCst)
                || !core.admitting.load(Ordering::SeqCst)
                || core.generation.load(Ordering::SeqCst) != generation
            {
                return Err("Transcription cancelled".into());
            }
            *core
                .current_directory
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(directory.clone());
            state.directory = Some(directory);
            state.model = Some(model);
            Ok(())
        })
        .await
        .map_err(|_| "Parakeet worker stopped unexpectedly".to_string())?
    }

    #[cfg(test)]
    pub async fn transcribe(
        &self,
        samples: Vec<f32>,
        cancel: Arc<AtomicBool>,
    ) -> Result<Output, String> {
        let core = Arc::clone(&self.core);
        let generation = core.generation.load(Ordering::SeqCst);
        if !core.admitting.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) {
            return Err("Transcription cancelled".into());
        }
        tokio::task::spawn_blocking(move || {
            let mut state = core.state.lock().unwrap_or_else(|error| error.into_inner());
            let cancelled = || {
                cancel.load(Ordering::SeqCst)
                    || core.cancel_worker.load(Ordering::SeqCst)
                    || !core.admitting.load(Ordering::SeqCst)
                    || core.generation.load(Ordering::SeqCst) != generation
            };
            if cancelled() {
                return Err("Transcription cancelled".into());
            }
            let model = state.model.as_mut().ok_or("Parakeet model is not loaded")?;
            let mut text = Vec::new();
            let mut words = Vec::new();
            for chunk in chunking::split(&samples) {
                if cancelled() {
                    return Err("Transcription cancelled".into());
                }
                let result = model.transcribe_chunk(&samples[chunk.start..chunk.end]);
                // Also reject the last chunk if cancellation arrived during inference.
                if cancelled() {
                    return Err("Transcription cancelled".into());
                }
                let result = result?;
                if !result.text.is_empty() {
                    text.push(result.text);
                }
                let offset = chunk.timestamp_offset_seconds();
                words.extend(result.words.into_iter().map(|mut word| {
                    word.start += offset;
                    word.end += offset;
                    word
                }));
            }
            if cancelled() {
                return Err("Transcription cancelled".into());
            }
            Ok(Output {
                text: text.join(" "),
                words,
            })
        })
        .await
        .map_err(|_| "Parakeet worker stopped unexpectedly".to_string())?
    }

    /// Stream normalized PCM into the same blocking worker that owns the model.
    /// Read 15 seconds at a time, retaining up to 30 seconds of lookahead for
    /// quiet-gap boundaries before falling back to a 15-second cut.
    #[cfg_attr(
        not(all(target_os = "windows", target_arch = "x86_64")),
        allow(dead_code)
    )]
    pub async fn transcribe_wav(
        &self,
        path: PathBuf,
        cancel: Arc<AtomicBool>,
    ) -> Result<(Output, f32), String> {
        let core = Arc::clone(&self.core);
        let generation = core.generation.load(Ordering::SeqCst);
        if !core.admitting.load(Ordering::SeqCst) || cancel.load(Ordering::SeqCst) {
            return Err("Transcription cancelled".into());
        }
        tokio::task::spawn_blocking(move || {
            let mut reader = hound::WavReader::open(path)
                .map_err(|_| "Cannot read the audio file for Parakeet transcription".to_string())?;
            let spec = reader.spec();
            if spec.channels != 1
                || spec.sample_rate != 16_000
                || spec.bits_per_sample != 16
                || spec.sample_format != hound::SampleFormat::Int
            {
                return Err("Parakeet requires normalized 16 kHz mono audio".into());
            }
            let mut state = core.state.lock().unwrap_or_else(|error| error.into_inner());
            let cancelled = || {
                cancel.load(Ordering::SeqCst)
                    || core.cancel_worker.load(Ordering::SeqCst)
                    || !core.admitting.load(Ordering::SeqCst)
                    || core.generation.load(Ordering::SeqCst) != generation
            };
            if cancelled() {
                return Err("Transcription cancelled".into());
            }
            let model = state.model.as_mut().ok_or("Parakeet model is not loaded")?;
            let mut text = Vec::new();
            let mut words = Vec::new();
            let mut sample_count = 0usize;
            let mut window_start = 0usize;
            let mut window = Vec::with_capacity(45 * chunking::SAMPLE_RATE);
            let mut samples = reader.samples::<i16>();
            let mut transcribe_chunk = |chunk: chunking::Chunk,
                                        window: &[f32],
                                        window_start: usize|
             -> Result<(), String> {
                if cancelled() {
                    return Err("Transcription cancelled".into());
                }
                let result = model.transcribe_chunk(&window[chunk.start..chunk.end])?;
                if cancelled() {
                    return Err("Transcription cancelled".into());
                }
                if !result.text.is_empty() {
                    text.push(result.text);
                }
                let offset = (window_start + chunk.start) as f32 / chunking::SAMPLE_RATE as f32;
                words.extend(result.words.into_iter().map(|mut word| {
                    word.start += offset;
                    word.end += offset;
                    word
                }));
                Ok(())
            };
            loop {
                if cancelled() {
                    return Err("Transcription cancelled".into());
                }
                let mut read_count = 0usize;
                for _ in 0..15 * chunking::SAMPLE_RATE {
                    if read_count.is_multiple_of(chunking::SAMPLE_RATE / 10) && cancelled() {
                        return Err("Transcription cancelled".into());
                    }
                    match samples.next() {
                        Some(Ok(value)) => {
                            window.push(value as f32 / 32768.0);
                            read_count += 1;
                        }
                        Some(Err(_)) => {
                            return Err(
                                "Cannot read the audio file for Parakeet transcription".into()
                            )
                        }
                        None => break,
                    }
                }
                sample_count += read_count;
                if read_count == 0 {
                    break;
                }
                if cancelled() {
                    return Err("Transcription cancelled".into());
                }
                if window.len() > 30 * chunking::SAMPLE_RATE {
                    let chunks = chunking::split(&window);
                    let remainder_start = chunks.last().expect("nonempty window").start;
                    for chunk in chunks.iter().take(chunks.len() - 1) {
                        transcribe_chunk(*chunk, &window, window_start)?;
                    }
                    window.drain(..remainder_start);
                    window_start += remainder_start;
                }
            }
            for chunk in chunking::split(&window) {
                transcribe_chunk(chunk, &window, window_start)?;
            }
            if cancelled() {
                return Err("Transcription cancelled".into());
            }
            Ok((
                Output {
                    text: text.join(" "),
                    words,
                },
                sample_count as f32 / chunking::SAMPLE_RATE as f32,
            ))
        })
        .await
        .map_err(|_| "Parakeet worker stopped unexpectedly".to_string())?
    }

    pub async fn unload(&self) {
        self.begin_unload();
        let _lifecycle = self.core.lifecycle.lock().await;
        let core = Arc::clone(&self.core);
        let _ = tokio::task::spawn_blocking(move || Self::clear_state(&core)).await;
    }

    /// Call only from a blocking thread, never from an async task.
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    pub fn unload_blocking(&self) {
        self.begin_unload();
        let _lifecycle = self.core.lifecycle.blocking_lock();
        Self::clear_state(&self.core);
    }

    fn begin_unload(&self) {
        // Signal before waiting for the lifecycle gate or model mutex: a worker
        // already in ORT observes cancellation at its next chunk boundary.
        self.core.admitting.store(false, Ordering::SeqCst);
        self.core.cancel_worker.store(true, Ordering::SeqCst);
        self.core.generation.fetch_add(1, Ordering::SeqCst);
        self.core.unload_epoch.fetch_add(1, Ordering::SeqCst);
    }

    fn clear_state(core: &Core<M>) {
        let mut state = core.state.lock().unwrap_or_else(|error| error.into_inner());
        state.model = None;
        state.directory = None;
        *core
            .current_directory
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parakeet::onnx::chunking::SAMPLE_RATE;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct Fake {
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicBool>,
        delay: Duration,
    }
    impl Model for Fake {
        fn transcribe_chunk(&mut self, samples: &[f32]) -> Result<Output, String> {
            assert!(
                !self.active.swap(true, Ordering::SeqCst),
                "workers overlapped"
            );
            assert!(samples.len() <= 30 * SAMPLE_RATE);
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            self.active.store(false, Ordering::SeqCst);
            Ok(Output {
                text: "word".into(),
                words: vec![Word {
                    text: "word".into(),
                    start: 0.0,
                    end: samples.len() as f32 / SAMPLE_RATE as f32,
                }],
            })
        }
    }
    fn slot(loads: Arc<AtomicUsize>, calls: Arc<AtomicUsize>, delay: Duration) -> Slot<Fake> {
        Slot::new(move |_| {
            loads.fetch_add(1, Ordering::SeqCst);
            Ok(Fake {
                calls: Arc::clone(&calls),
                active: Arc::new(AtomicBool::new(false)),
                delay,
            })
        })
    }
    fn token() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    fn wav_file_with_gap(seconds: usize, gap: std::ops::Range<usize>) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE as u32,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(file.path(), spec).unwrap();
        for sample in 0..seconds * SAMPLE_RATE {
            writer
                .write_sample(if gap.contains(&sample) { 0i16 } else { 100i16 })
                .unwrap();
        }
        writer.finalize().unwrap();
        file
    }

    fn wav_file(seconds: usize) -> tempfile::NamedTempFile {
        wav_file_with_gap(seconds, 0..0)
    }

    fn assert_contiguous(words: &[Word], duration: f32) {
        assert_eq!(words.first().map(|word| word.start), Some(0.0));
        assert!(words
            .windows(2)
            .all(|pair| (pair[0].end - pair[1].start).abs() < 0.001));
        assert!((words.last().unwrap().end - duration).abs() < 0.001);
    }

    #[tokio::test]
    async fn wav_worker_uses_bounded_windows_and_offsets() {
        let calls = Arc::new(AtomicUsize::new(0));
        let slot = slot(
            Arc::new(AtomicUsize::new(0)),
            Arc::clone(&calls),
            Duration::ZERO,
        );
        slot.load("model".into(), token()).await.unwrap();
        let file = wav_file(31);
        let (output, duration) = slot
            .transcribe_wav(file.path().into(), token())
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(duration, 31.0);
        assert_eq!(
            output
                .words
                .iter()
                .map(|word| word.start)
                .collect::<Vec<_>>(),
            vec![0.0, 15.0]
        );
        assert_contiguous(&output.words, duration);
    }

    #[tokio::test]
    async fn wav_worker_cuts_in_quiet_gap_and_transcribes_remainder() {
        let calls = Arc::new(AtomicUsize::new(0));
        let slot = slot(
            Arc::new(AtomicUsize::new(0)),
            Arc::clone(&calls),
            Duration::ZERO,
        );
        slot.load("model".into(), token()).await.unwrap();
        let file = wav_file_with_gap(40, 18 * SAMPLE_RATE..20 * SAMPLE_RATE);
        let (output, duration) = slot
            .transcribe_wav(file.path().into(), token())
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(duration, 40.0);
        assert!(output.words[0].end > 18.0 && output.words[0].end < 20.0);
        assert_contiguous(&output.words, duration);
    }

    #[tokio::test]
    async fn wav_worker_continuous_fallback_and_short_eof() {
        let calls = Arc::new(AtomicUsize::new(0));
        let slot = slot(
            Arc::new(AtomicUsize::new(0)),
            Arc::clone(&calls),
            Duration::ZERO,
        );
        slot.load("model".into(), token()).await.unwrap();
        let file = wav_file(100);
        let (output, duration) = slot
            .transcribe_wav(file.path().into(), token())
            .await
            .unwrap();
        assert_eq!(duration, 100.0);
        assert_eq!(calls.load(Ordering::SeqCst), 6);
        assert_eq!(output.words[0].end, 15.0);
        assert_contiguous(&output.words, duration);

        let file = wav_file(7);
        let (output, duration) = slot
            .transcribe_wav(file.path().into(), token())
            .await
            .unwrap();
        assert_eq!(duration, 7.0);
        assert_eq!(output.words.len(), 1);
        assert_contiguous(&output.words, duration);
    }

    #[tokio::test]
    async fn wav_worker_checks_cancellation_after_inference() {
        let calls = Arc::new(AtomicUsize::new(0));
        let slot = slot(
            Arc::new(AtomicUsize::new(0)),
            Arc::clone(&calls),
            Duration::from_millis(70),
        );
        slot.load("model".into(), token()).await.unwrap();
        let file = wav_file(31);
        let cancel = token();
        let future = slot.transcribe_wav(file.path().into(), Arc::clone(&cancel));
        let cancel_task = async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
            cancel.store(true, Ordering::SeqCst);
        };
        let (result, ()) = tokio::join!(future, cancel_task);
        assert_eq!(result.unwrap_err(), "Transcription cancelled");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn idempotent_load_and_serialized_workers() {
        let loads = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let slot = slot(
            Arc::clone(&loads),
            Arc::clone(&calls),
            Duration::from_millis(5),
        );
        slot.load("model".into(), token()).await.unwrap();
        slot.load("model".into(), token()).await.unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        let (a, b) = tokio::join!(
            slot.transcribe(vec![0.1], token()),
            slot.transcribe(vec![0.1], token())
        );
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cancel_between_chunks_and_reject_late_result() {
        let slot = slot(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(AtomicUsize::new(0)),
            Duration::from_millis(60),
        );
        slot.load("model".into(), token()).await.unwrap();
        let cancel = token();
        let future = slot.transcribe(vec![0.1; 31 * SAMPLE_RATE], Arc::clone(&cancel));
        let cancel_task = async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            cancel.store(true, Ordering::SeqCst);
        };
        let (result, ()) = tokio::join!(future, cancel_task);
        assert_eq!(result.unwrap_err(), "Transcription cancelled");
    }

    #[tokio::test]
    async fn unload_supersedes_in_flight_load() {
        let started = Arc::new(AtomicBool::new(false));
        let started_for_factory = Arc::clone(&started);
        let slot = Slot::new(move |_| {
            started_for_factory.store(true, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(80));
            Ok(Fake {
                calls: Arc::new(AtomicUsize::new(0)),
                active: Arc::new(AtomicBool::new(false)),
                delay: Duration::ZERO,
            })
        });
        let load = slot.load("model".into(), token());
        let unload = async {
            while !started.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            slot.unload().await;
        };
        let (result, ()) = tokio::join!(load, unload);
        assert_eq!(result.unwrap_err(), "Transcription cancelled");
        assert_eq!(
            slot.transcribe(vec![0.1], token()).await.unwrap_err(),
            "Transcription cancelled"
        );
    }

    #[tokio::test]
    async fn unload_waits_for_worker_and_discards_its_result() {
        let calls = Arc::new(AtomicUsize::new(0));
        let slot = slot(
            Arc::new(AtomicUsize::new(0)),
            Arc::clone(&calls),
            Duration::from_millis(80),
        );
        slot.load("model".into(), token()).await.unwrap();
        let future = slot.transcribe(vec![0.1], token());
        let unload = async {
            while calls.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
            slot.unload().await;
        };
        let (result, ()) = tokio::join!(future, unload);
        assert_eq!(result.unwrap_err(), "Transcription cancelled");
        assert_eq!(
            slot.transcribe(vec![0.1], token()).await.unwrap_err(),
            "Transcription cancelled"
        );
    }
}
