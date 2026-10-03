//! Debounce before expensive snapshot/native menu work.
use std::sync::atomic::{AtomicU64, Ordering};
pub(crate) async fn wait_for_latest(counter: &AtomicU64, generation: u64) -> bool {
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    generation == counter.load(Ordering::SeqCst)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn burst_builds_only_the_latest_refresh() {
        let counter = AtomicU64::new(0);
        let refresh = || async {
            let generation = counter.fetch_add(1, Ordering::SeqCst) + 1;
            wait_for_latest(&counter, generation).await
        };
        let (first, second, third) = tokio::join!(refresh(), refresh(), refresh());
        assert_eq!([first, second, third], [false, false, true]);
    }
}
