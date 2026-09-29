use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::transcription::stream::StreamSessionGate;
use crate::utils::logger::log_performance;

pub const STREAM_QUEUE_CAPACITY: usize = 256;
pub const STREAM_RECYCLE_CAPACITY: usize = STREAM_QUEUE_CAPACITY + 4;

#[derive(Debug)]
pub enum StreamTapMsg {
    Frame(Vec<i16>),
    Finalize,
    #[allow(dead_code)] // Future streaming cancel wiring slice will construct this.
    Cancel,
}

pub struct StreamTapRt {
    tx: SyncSender<StreamTapMsg>,
    pool_rx: Receiver<Vec<i16>>,
    pool_tx: SyncSender<Vec<i16>>,
    dropped: Arc<AtomicU64>,
}

pub trait StreamTapSink: Send {
    fn send_frame(&mut self, samples: &[i16]);
    /// `dropped_frames` is the tap's RT-side drop count for this recording (pool
    /// exhaustion / full queue). Preview-only sinks may ignore it; a sink whose
    /// final text is result-AUTHORITATIVE (Soniox, plan 043b) must treat any drop
    /// as authority-invalidating — the stream saw incomplete audio, so the
    /// complete-WAV fallback path owns the pasted result.
    fn finalize(&mut self, dropped_frames: u64) -> Option<String>;
    fn cancel(&mut self);
}

pub type StreamTapSinkFactory =
    Arc<dyn Fn(u32, u16) -> Option<Box<dyn StreamTapSink>> + Send + Sync>;

fn cancel_sink(sink: &mut Option<Box<dyn StreamTapSink>>) {
    if let Some(sink) = sink.as_mut() {
        sink.cancel();
    }
}

fn finalize_sink(sink: &mut Option<Box<dyn StreamTapSink>>, dropped_frames: u64) {
    if let Some(sink) = sink.as_mut() {
        let _ = sink.finalize(dropped_frames);
    }
}

impl StreamTapRt {
    #[allow(dead_code)] // Future streaming diagnostics slice will read this accessor.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

pub struct StreamTapHandle {
    rt: StreamTapRt,
    tx: SyncSender<StreamTapMsg>,
    finalize_flag: Arc<AtomicBool>,
    worker: thread::JoinHandle<StreamTapWorkerSummary>,
}

impl StreamTapHandle {
    pub fn into_rt(self) -> (StreamTapRt, StreamTapFinalizer) {
        (
            self.rt,
            StreamTapFinalizer {
                tx: self.tx,
                finalize_flag: self.finalize_flag,
                worker: self.worker,
            },
        )
    }
}

pub struct StreamTapFinalizer {
    tx: SyncSender<StreamTapMsg>,
    finalize_flag: Arc<AtomicBool>,
    worker: thread::JoinHandle<StreamTapWorkerSummary>,
}

impl StreamTapFinalizer {
    pub fn finalize(self) -> thread::JoinHandle<StreamTapWorkerSummary> {
        self.finalize_flag.store(true, Ordering::SeqCst);
        let _ = self.tx.try_send(StreamTapMsg::Finalize);
        drop(self.tx);
        self.worker
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamTapWorkerSummary {
    pub generation: u64,
    pub frames: u64,
    pub samples: u64,
    pub dropped: u64,
    pub cancelled: bool,
    pub stale: bool,
}

impl StreamTapWorkerSummary {
    fn metadata(&self) -> String {
        format!(
            "frames={} samples={} dropped={} generation={} cancelled={} stale={}",
            self.frames, self.samples, self.dropped, self.generation, self.cancelled, self.stale
        )
    }
}

#[cfg(test)]
pub fn spawn_noop_worker(
    generation: u64,
    chunk_capacity: usize,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    sink: Option<Box<dyn StreamTapSink>>,
) -> StreamTapHandle {
    spawn_noop_worker_with_factory(generation, chunk_capacity, cancelled, sink, None)
}

pub fn spawn_noop_worker_with_factory(
    generation: u64,
    chunk_capacity: usize,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    sink: Option<Box<dyn StreamTapSink>>,
    sink_after_play: Option<(StreamTapSinkFactory, u32, u16, Receiver<()>)>,
) -> StreamTapHandle {
    let (tx, rx) = mpsc::sync_channel::<StreamTapMsg>(STREAM_QUEUE_CAPACITY);
    let (pool_tx, pool_rx) = mpsc::sync_channel::<Vec<i16>>(STREAM_RECYCLE_CAPACITY);

    for _ in 0..STREAM_RECYCLE_CAPACITY {
        let _ = pool_tx.send(Vec::with_capacity(chunk_capacity));
    }

    let dropped = Arc::new(AtomicU64::new(0));
    let finalize_flag = Arc::new(AtomicBool::new(false));
    let worker_dropped = dropped.clone();
    let worker_finalize_flag = finalize_flag.clone();
    let worker_pool_tx = pool_tx.clone();
    let worker_tx = tx.clone();
    let worker = thread::spawn(move || {
        let sink = if let Some((factory, sample_rate, channels, play_rx)) = sink_after_play {
            if play_rx.recv().is_err() {
                return StreamTapWorkerSummary {
                    generation,
                    frames: 0,
                    samples: 0,
                    dropped: 0,
                    cancelled: true,
                    stale: false,
                };
            }
            factory(sample_rate, channels)
        } else {
            sink
        };
        run_noop_worker(
            rx,
            worker_pool_tx,
            worker_dropped,
            worker_finalize_flag,
            generation,
            cancelled,
            sink,
        )
    });

    StreamTapHandle {
        rt: StreamTapRt {
            tx: worker_tx,
            pool_rx,
            pool_tx,
            dropped,
        },
        tx,
        finalize_flag,
        worker,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn maybe_spawn_noop_worker_after_play(
    enabled: bool,
    generation: u64,
    chunk_capacity: usize,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    sink_factory: Option<StreamTapSinkFactory>,
    sample_rate: u32,
    channels: u16,
    play_rx: Receiver<()>,
) -> Option<StreamTapHandle> {
    enabled.then(|| {
        spawn_noop_worker_with_factory(
            generation,
            chunk_capacity,
            cancelled,
            None,
            sink_factory.map(|factory| (factory, sample_rate, channels, play_rx)),
        )
    })
}

#[cfg(test)]
pub fn maybe_spawn_noop_worker(
    enabled: bool,
    generation: u64,
    chunk_capacity: usize,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    sink: Option<Box<dyn StreamTapSink>>,
) -> Option<StreamTapHandle> {
    enabled.then(|| spawn_noop_worker(generation, chunk_capacity, cancelled, sink))
}

pub fn enqueue_frame_rt(tap: &StreamTapRt, i16_samples: &[i16]) {
    let mut chunk = match tap.pool_rx.try_recv() {
        Ok(chunk) => chunk,
        Err(TryRecvError::Empty | TryRecvError::Disconnected) => {
            tap.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    chunk.clear();
    if i16_samples.len() > chunk.capacity() {
        tap.dropped.fetch_add(1, Ordering::Relaxed);
        let _ = tap.pool_tx.try_send(chunk);
        return;
    }

    chunk.extend_from_slice(i16_samples);
    match tap.tx.try_send(StreamTapMsg::Frame(chunk)) {
        Ok(()) => {}
        Err(TrySendError::Full(StreamTapMsg::Frame(mut chunk)))
        | Err(TrySendError::Disconnected(StreamTapMsg::Frame(mut chunk))) => {
            tap.dropped.fetch_add(1, Ordering::Relaxed);
            chunk.clear();
            let _ = tap.pool_tx.try_send(chunk);
        }
        Err(TrySendError::Full(StreamTapMsg::Finalize | StreamTapMsg::Cancel))
        | Err(TrySendError::Disconnected(StreamTapMsg::Finalize | StreamTapMsg::Cancel)) => {
            tap.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn run_noop_worker(
    rx: Receiver<StreamTapMsg>,
    pool_tx: SyncSender<Vec<i16>>,
    dropped: Arc<AtomicU64>,
    finalize_flag: Arc<AtomicBool>,
    generation: u64,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    sink: Option<Box<dyn StreamTapSink>>,
) -> StreamTapWorkerSummary {
    run_noop_worker_observed(
        NoopWorkerContext {
            rx,
            pool_tx,
            dropped,
            finalize_flag,
            generation,
            cancelled,
            sink,
            log_summary: true,
        },
        |_| {},
    )
}

struct NoopWorkerContext {
    rx: Receiver<StreamTapMsg>,
    pool_tx: SyncSender<Vec<i16>>,
    dropped: Arc<AtomicU64>,
    finalize_flag: Arc<AtomicBool>,
    generation: u64,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    sink: Option<Box<dyn StreamTapSink>>,
    log_summary: bool,
}

fn run_noop_worker_observed<F>(
    context: NoopWorkerContext,
    mut on_frame: F,
) -> StreamTapWorkerSummary
where
    F: FnMut(&[i16]),
{
    let NoopWorkerContext {
        rx,
        pool_tx,
        dropped,
        finalize_flag,
        generation,
        cancelled,
        mut sink,
        log_summary,
    } = context;

    let started = Instant::now();
    let mut gate = StreamSessionGate::new(generation);
    let mut frames = 0_u64;
    let mut samples = 0_u64;
    let mut cancelled_seen = false;
    let mut stale_seen = false;

    loop {
        let msg = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Disconnected) => {
                stale_seen |= crate::commands::audio::recording_generation_is_stale(generation);
                cancelled_seen |= (cancelled)();
                if stale_seen || cancelled_seen || !finalize_flag.load(Ordering::SeqCst) {
                    cancel_sink(&mut sink);
                } else {
                    finalize_sink(&mut sink, dropped.load(Ordering::Relaxed));
                }
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                stale_seen |= crate::commands::audio::recording_generation_is_stale(generation);
                let now_cancelled = (cancelled)();
                cancelled_seen |= now_cancelled;
                if stale_seen || now_cancelled {
                    cancel_sink(&mut sink);
                    break;
                }
                if finalize_flag.load(Ordering::SeqCst) {
                    finalize_sink(&mut sink, dropped.load(Ordering::Relaxed));
                    break;
                }
                continue;
            }
        };

        stale_seen |= crate::commands::audio::recording_generation_is_stale(generation);
        if stale_seen || (cancelled)() {
            cancelled_seen |= (cancelled)();
            match msg {
                StreamTapMsg::Frame(mut chunk) => {
                    chunk.clear();
                    let _ = pool_tx.send(chunk);
                }
                StreamTapMsg::Finalize => {
                    cancel_sink(&mut sink);
                    break;
                }
                StreamTapMsg::Cancel => {
                    cancelled_seen = true;
                    cancel_sink(&mut sink);
                    break;
                }
            }
            continue;
        }

        match msg {
            StreamTapMsg::Frame(mut chunk) => {
                let event = crate::transcription::stream::TranscriptionStreamEvent::Partial {
                    session_id: generation,
                    revision: frames + 1,
                    committed: String::new(),
                    tentative: String::new(),
                };
                if matches!(
                    gate.admit(&event),
                    crate::transcription::stream::Admit::Accept
                ) {
                    on_frame(&chunk);
                    if let Some(sink) = sink.as_mut() {
                        sink.send_frame(&chunk);
                    }
                    frames += 1;
                    samples += chunk.len() as u64;
                }
                chunk.clear();
                let _ = pool_tx.send(chunk);
            }
            StreamTapMsg::Finalize => {
                finalize_sink(&mut sink, dropped.load(Ordering::Relaxed));
                break;
            }
            StreamTapMsg::Cancel => {
                cancelled_seen = true;
                cancel_sink(&mut sink);
                break;
            }
        }
    }

    let summary = StreamTapWorkerSummary {
        generation,
        frames: if cancelled_seen || stale_seen {
            0
        } else {
            frames
        },
        samples: if cancelled_seen || stale_seen {
            0
        } else {
            samples
        },
        dropped: dropped.load(Ordering::Relaxed),
        cancelled: cancelled_seen,
        stale: stale_seen,
    };

    if log_summary {
        log_performance(
            "STREAM_TAP",
            started.elapsed().as_millis() as u64,
            Some(&summary.metadata()),
        );
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::audio::begin_recording_generation;
    use std::sync::atomic::AtomicUsize;

    fn not_cancelled() -> Arc<dyn Fn() -> bool + Send + Sync> {
        Arc::new(|| false)
    }

    #[test]
    fn blocking_sink_factory_starts_only_after_play_and_does_not_block_it() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation = begin_recording_generation();
        let played = Arc::new(AtomicBool::new(false));
        let release_factory = Arc::new(AtomicBool::new(false));
        let (entered_tx, entered_rx) = mpsc::channel();
        let played_in_factory = played.clone();
        let release_in_factory = release_factory.clone();
        let factory: StreamTapSinkFactory = Arc::new(move |_, _| {
            assert!(played_in_factory.load(Ordering::SeqCst));
            let _ = entered_tx.send(());
            while !release_in_factory.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(1));
            }
            None
        });
        let (play_tx, play_rx) = mpsc::channel();
        let handle = maybe_spawn_noop_worker_after_play(
            true,
            generation,
            4,
            not_cancelled(),
            Some(factory),
            16_000,
            1,
            play_rx,
        )
        .unwrap();
        assert!(entered_rx.try_recv().is_err());
        played.store(true, Ordering::SeqCst);
        play_tx.send(()).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        // The factory is still blocked here, yet the simulated play completed.
        assert!(played.load(Ordering::SeqCst));
        release_factory.store(true, Ordering::SeqCst);
        let (_, finalizer) = handle.into_rt();
        finalizer.finalize().join().unwrap();
    }

    struct CountingSink {
        finalized: Arc<AtomicUsize>,
        cancelled: Arc<AtomicUsize>,
    }

    impl StreamTapSink for CountingSink {
        fn send_frame(&mut self, _samples: &[i16]) {}

        fn finalize(&mut self, _dropped_frames: u64) -> Option<String> {
            self.finalized.fetch_add(1, Ordering::SeqCst);
            Some(String::new())
        }

        fn cancel(&mut self) {
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn counting_sink() -> (Box<dyn StreamTapSink>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let finalized = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicUsize::new(0));
        (
            Box::new(CountingSink {
                finalized: finalized.clone(),
                cancelled: cancelled.clone(),
            }),
            finalized,
            cancelled,
        )
    }

    fn test_context(
        rx: Receiver<StreamTapMsg>,
        pool_tx: SyncSender<Vec<i16>>,
        dropped: Arc<AtomicU64>,
        finalize_flag: Arc<AtomicBool>,
        generation: u64,
    ) -> NoopWorkerContext {
        NoopWorkerContext {
            rx,
            pool_tx,
            dropped,
            finalize_flag,
            generation,
            cancelled: not_cancelled(),
            sink: None,
            log_summary: false,
        }
    }

    #[test]
    fn fifo_ordering_frames_before_finalize() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation = begin_recording_generation();
        let (tx, rx) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
        let (pool_tx, _pool_rx) = mpsc::sync_channel(STREAM_RECYCLE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let finalize_flag = Arc::new(AtomicBool::new(false));

        tx.try_send(StreamTapMsg::Frame(vec![1, 2])).unwrap();
        tx.try_send(StreamTapMsg::Frame(vec![3])).unwrap();
        tx.try_send(StreamTapMsg::Frame(vec![4, 5, 6])).unwrap();
        finalize_flag.store(true, Ordering::SeqCst);
        tx.try_send(StreamTapMsg::Finalize).unwrap();
        drop(tx);

        let mut observed = Vec::new();
        let summary = run_noop_worker_observed(
            test_context(rx, pool_tx, dropped, finalize_flag, generation),
            |frame| observed.push(frame.to_vec()),
        );

        assert_eq!(observed, vec![vec![1, 2], vec![3], vec![4, 5, 6]]);
        assert_eq!(summary.frames, 3);
        assert_eq!(summary.samples, 6);
    }

    #[test]
    fn queue_overflow_drops_never_block() {
        let (tx, _rx) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
        let (pool_tx, pool_rx) = mpsc::sync_channel(STREAM_RECYCLE_CAPACITY);
        let (writer_tx, writer_rx) = mpsc::sync_channel::<Vec<i16>>(1);
        let dropped = Arc::new(AtomicU64::new(0));
        for _ in 0..STREAM_RECYCLE_CAPACITY {
            pool_tx.send(Vec::with_capacity(8)).unwrap();
        }
        for _ in 0..STREAM_QUEUE_CAPACITY {
            tx.try_send(StreamTapMsg::Frame(Vec::new())).unwrap();
        }
        writer_tx.try_send(vec![9, 9, 9]).unwrap();
        let tap = StreamTapRt {
            tx,
            pool_rx,
            pool_tx,
            dropped,
        };

        enqueue_frame_rt(&tap, &[1, 2, 3]);

        assert_eq!(tap.dropped(), 1);
        assert_eq!(writer_rx.try_recv().unwrap(), vec![9, 9, 9]);
    }

    #[test]
    fn cancel_drains_and_discards_without_content_summary() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation = begin_recording_generation();
        let (tx, rx) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
        let (pool_tx, _pool_rx) = mpsc::sync_channel(STREAM_RECYCLE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let finalize_flag = Arc::new(AtomicBool::new(false));

        tx.try_send(StreamTapMsg::Frame(vec![1, 2, 3])).unwrap();
        tx.try_send(StreamTapMsg::Cancel).unwrap();
        drop(tx);

        let summary = run_noop_worker_observed(
            test_context(rx, pool_tx, dropped, finalize_flag, generation),
            |_| {},
        );

        assert!(summary.cancelled);
        assert_eq!(summary.frames, 0);
        assert_eq!(summary.samples, 0);
    }

    #[test]
    fn pool_recycle_round_trip_and_exhaustion_drops_without_allocating() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation = begin_recording_generation();
        let handle = spawn_noop_worker(generation, 4, not_cancelled(), None);
        let (rt, finalizer) = handle.into_rt();

        enqueue_frame_rt(&rt, &[1, 2, 3, 4]);
        let worker = finalizer.finalize();
        let summary = worker.join().unwrap();

        assert_eq!(summary.frames, 1);
        assert_eq!(summary.samples, 4);
        assert_eq!(rt.pool_rx.try_iter().count(), STREAM_RECYCLE_CAPACITY);

        let (tx, _rx) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
        let (pool_tx, pool_rx) = mpsc::sync_channel(STREAM_RECYCLE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let exhausted = StreamTapRt {
            tx,
            pool_rx,
            pool_tx,
            dropped,
        };
        enqueue_frame_rt(&exhausted, &[1]);
        assert_eq!(exhausted.dropped(), 1);
    }

    #[test]
    fn flag_off_is_inert_and_spawns_no_worker() {
        assert!(maybe_spawn_noop_worker(false, 0, 4, not_cancelled(), None).is_none());
    }

    #[test]
    fn stale_generation_discards_frames() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let stale_generation = begin_recording_generation();
        let _new_generation = begin_recording_generation();
        let (tx, rx) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
        let (pool_tx, _pool_rx) = mpsc::sync_channel(STREAM_RECYCLE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let finalize_flag = Arc::new(AtomicBool::new(true));

        tx.try_send(StreamTapMsg::Frame(vec![1, 2, 3])).unwrap();
        tx.try_send(StreamTapMsg::Finalize).unwrap();
        drop(tx);

        let summary = run_noop_worker_observed(
            test_context(rx, pool_tx, dropped, finalize_flag, stale_generation),
            |_| {},
        );

        assert!(summary.stale);
        assert_eq!(summary.frames, 0);
        assert_eq!(summary.samples, 0);
    }

    #[test]
    fn cancelled_finalize_cancels_sink_instead_of_abandoning_session() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation = begin_recording_generation();
        let (tx, rx) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
        let (pool_tx, _pool_rx) = mpsc::sync_channel(STREAM_RECYCLE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let finalize_flag = Arc::new(AtomicBool::new(true));
        let (sink, finalized, cancelled) = counting_sink();

        tx.try_send(StreamTapMsg::Finalize).unwrap();
        drop(tx);

        let context = NoopWorkerContext {
            rx,
            pool_tx,
            dropped,
            finalize_flag,
            generation,
            cancelled: Arc::new(|| true),
            sink: Some(sink),
            log_summary: false,
        };
        let summary = run_noop_worker_observed(context, |_| {});

        assert!(summary.cancelled);
        assert_eq!(cancelled.load(Ordering::SeqCst), 1);
        assert_eq!(finalized.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn disconnected_without_finalize_cancels_sink() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation = begin_recording_generation();
        let (tx, rx) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
        let (pool_tx, _pool_rx) = mpsc::sync_channel(STREAM_RECYCLE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let finalize_flag = Arc::new(AtomicBool::new(false));
        let (sink, finalized, cancelled) = counting_sink();
        drop(tx);

        let context = NoopWorkerContext {
            rx,
            pool_tx,
            dropped,
            finalize_flag,
            generation,
            cancelled: not_cancelled(),
            sink: Some(sink),
            log_summary: false,
        };
        let summary = run_noop_worker_observed(context, |_| {});

        assert!(!summary.cancelled);
        assert_eq!(cancelled.load(Ordering::SeqCst), 1);
        assert_eq!(finalized.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn disconnected_after_finalize_flag_finalizes_sink() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let generation = begin_recording_generation();
        let (tx, rx) = mpsc::sync_channel(STREAM_QUEUE_CAPACITY);
        let (pool_tx, _pool_rx) = mpsc::sync_channel(STREAM_RECYCLE_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let finalize_flag = Arc::new(AtomicBool::new(true));
        let (sink, finalized, cancelled) = counting_sink();
        drop(tx);

        let context = NoopWorkerContext {
            rx,
            pool_tx,
            dropped,
            finalize_flag,
            generation,
            cancelled: not_cancelled(),
            sink: Some(sink),
            log_summary: false,
        };
        let summary = run_noop_worker_observed(context, |_| {});

        assert!(!summary.cancelled);
        assert_eq!(finalized.load(Ordering::SeqCst), 1);
        assert_eq!(cancelled.load(Ordering::SeqCst), 0);
    }
}
