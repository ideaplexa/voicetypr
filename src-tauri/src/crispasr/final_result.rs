use super::messages::NativeTranscript;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::oneshot;

type FinalReceiver = oneshot::Receiver<Result<NativeTranscript, String>>;
struct Entry {
    model: String,
    language: String,
    receiver: FinalReceiver,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct FinalResults(Mutex<HashMap<u64, Entry>>);

impl FinalResults {
    pub fn register(
        &self,
        generation: u64,
        model: String,
        language: String,
        receiver: FinalReceiver,
        cancelled: Arc<AtomicBool>,
    ) -> bool {
        let Ok(mut entries) = self.0.lock() else {
            return false;
        };
        entries.retain(|stored, entry| {
            if *stored < generation {
                entry.cancelled.store(true, Ordering::SeqCst);
                false
            } else {
                true
            }
        });
        entries.insert(
            generation,
            Entry {
                model,
                language,
                receiver,
                cancelled,
            },
        );
        true
    }
    pub async fn take(
        &self,
        generation: u64,
        model: &str,
        language: &str,
    ) -> Option<NativeTranscript> {
        // Remove only this take, even if an old task wakes after a newer one.
        let entry = self.0.lock().ok()?.remove(&generation)?;
        if entry.model != model || entry.language != language {
            entry.cancelled.store(true, Ordering::SeqCst);
            return None;
        }
        let result = tokio::time::timeout(Duration::from_secs(180), entry.receiver).await;
        match result {
            Ok(Ok(Ok(transcript))) => Some(transcript),
            _ => {
                // Release the sidecar lease before complete-file fallback. A
                // detached finalizer must not hold its mutex after this wait ends.
                entry.cancelled.store(true, Ordering::SeqCst);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn stale_generation_cannot_consume_a_newer_final() {
        let finals = FinalResults::default();
        let (sender, receiver) = oneshot::channel();
        assert!(finals.register(
            2,
            "model".into(),
            "auto".into(),
            receiver,
            Arc::new(AtomicBool::new(false))
        ));
        assert!(finals.take(1, "model", "auto").await.is_none());
        assert!(sender
            .send(Ok(NativeTranscript {
                text: "text".into(),
                language: None,
                segments: vec![],
                samples: 16000,
                processing_ms: 5
            }))
            .is_ok());
        assert_eq!(
            finals.take(2, "model", "auto").await.unwrap().samples,
            16000
        );
    }
    #[tokio::test]
    async fn changed_model_or_language_rejects_stream_authority() {
        for (model, language) in [("other", "auto"), ("model", "fr")] {
            let finals = FinalResults::default();
            let (_sender, receiver) = oneshot::channel();
            let cancelled = Arc::new(AtomicBool::new(false));
            finals.register(
                2,
                "model".into(),
                "auto".into(),
                receiver,
                cancelled.clone(),
            );
            assert!(finals.take(2, model, language).await.is_none());
            assert!(cancelled.load(Ordering::SeqCst));
        }
    }

    #[tokio::test]
    async fn new_generation_cancels_and_releases_an_unclaimed_old_result() {
        let finals = FinalResults::default();
        let (old_sender, receiver) = oneshot::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        finals.register(
            2,
            "model".into(),
            "auto".into(),
            receiver,
            cancelled.clone(),
        );
        let (_new_sender, receiver) = oneshot::channel();
        finals.register(
            3,
            "model".into(),
            "auto".into(),
            receiver,
            Arc::new(AtomicBool::new(false)),
        );
        assert!(cancelled.load(Ordering::SeqCst));
        assert!(old_sender.is_closed());
        assert!(finals.take(2, "model", "auto").await.is_none());
    }
}
