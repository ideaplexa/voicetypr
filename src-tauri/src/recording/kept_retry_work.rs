//! Native decoding boundary; ownership and cancellation stay in the real retry.
use super::*;
use crate::commands::recording_runtime::RecordingRuntime;

pub(super) async fn run<R: RecordingRuntime>(
    app: &AppHandle<R>,
    lease: &Lease,
    generation: u64,
    engine: Option<String>,
    attempted: &mut Option<RetrySelection>,
) -> Result<(), String> {
    #[cfg(test)]
    if let Some(work) = app.try_state::<super::lifecycle_tests::Work>() {
        *attempted = Some(work.selection.clone());
        work.started.notify_one();
        work.finish.notified().await;
        return work.result.lock().unwrap().take().unwrap();
    }
    let native = R::native(app).ok_or("Native retry unavailable")?;
    retry_inner(native, lease, generation, engine, attempted).await
}
