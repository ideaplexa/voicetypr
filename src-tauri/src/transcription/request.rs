use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::provider_capabilities::ProviderEngine;
use crate::transcription::{TranscriptionSource, TranscriptionTask};

#[derive(Debug, Clone)]
pub enum TranscriptionAudio {
    Path {
        path: PathBuf,
        format_hint: Option<AudioFormatHint>,
        cleanup: CleanupPolicy,
    },
    Bytes {
        bytes: Vec<u8>,
        format_hint: Option<AudioFormatHint>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioFormatHint {
    Wav,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupPolicy {
    CallerOwns,
    DeleteAfterAttempt,
    PreserveOnRetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineSelection {
    Explicit {
        engine: ProviderEngine,
        model: String,
    },
    /// Remote server inbound (Stage 4) snapshots its own shared engine/model.
    HostDefault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeoutPolicy {
    Interactive,
    Upload,
    Explicit(Duration),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestTerm {
    pub text: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestCorrection {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestContext {
    pub terms: Vec<RequestTerm>,
    pub corrections: Vec<RequestCorrection>,
    pub max_bytes: u32,
}

impl RequestContext {
    /// Prunes context deterministically by input order until its JSON byte size is within
    /// `max_bytes`: trailing corrections are removed before trailing terms in each pass,
    /// with a size check after every removal. A `max_bytes` value of `0` disables pruning.
    /// If the cleared context still exceeds the cap, both vectors remain empty.
    pub fn prune_to(&mut self, max_bytes: u32) {
        if max_bytes == 0 || self.serialized_len() <= max_bytes {
            return;
        }

        loop {
            let mut removed = false;

            if self.corrections.pop().is_some() {
                removed = true;
                if self.serialized_len() <= max_bytes {
                    return;
                }
            }

            if self.terms.pop().is_some() {
                removed = true;
                if self.serialized_len() <= max_bytes {
                    return;
                }
            }

            if !removed {
                return;
            }
        }
    }

    fn serialized_len(&self) -> u32 {
        serde_json::to_vec(self)
            .expect("RequestContext serialization should not fail")
            .len() as u32
    }
}

/// Typed cancellation (replaces today's string-matched "Transcription cancelled").
/// Backed by Arc<AtomicBool> to match the existing `cancel_flag` closures.
#[derive(Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn from_arc(flag: Arc<AtomicBool>) -> Self {
        Self(flag)
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Inner flag for delegating to APIs that take `Option<Arc<AtomicBool>>`
    /// (e.g. the Parakeet manager).
    pub fn as_arc(&self) -> Arc<AtomicBool> {
        self.0.clone()
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    pub source: TranscriptionSource,
    pub audio: TranscriptionAudio,
    pub engine: EngineSelection,
    pub spoken_language: Option<String>,
    pub task: TranscriptionTask,
    pub context: RequestContext,
    pub timeout: TimeoutPolicy,
    pub cancellation: CancellationToken,
    pub initial_prompt: Option<String>,
    pub audio_ctx: Option<i32>,
    pub speed_mode_override: Option<bool>,
}

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CancellationToken")
            .field(&self.is_cancelled())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> RequestContext {
        RequestContext {
            terms: vec![
                RequestTerm {
                    text: "alpha".to_string(),
                    aliases: vec!["a".to_string()],
                },
                RequestTerm {
                    text: "bravo".to_string(),
                    aliases: vec!["b".to_string()],
                },
            ],
            corrections: vec![
                RequestCorrection {
                    from: "teh".to_string(),
                    to: "the".to_string(),
                },
                RequestCorrection {
                    from: "recieve".to_string(),
                    to: "receive".to_string(),
                },
            ],
            max_bytes: 4096,
        }
    }

    fn serialized_len(context: &RequestContext) -> u32 {
        serde_json::to_vec(context).unwrap().len() as u32
    }

    #[test]
    fn constructs_transcription_request() {
        let request = TranscriptionRequest {
            source: TranscriptionSource::AudioBytes,
            audio: TranscriptionAudio::Bytes {
                bytes: vec![0, 1, 2, 3],
                format_hint: Some(AudioFormatHint::Wav),
            },
            engine: EngineSelection::Explicit {
                engine: ProviderEngine::Whisper,
                model: "base.en".to_string(),
            },
            spoken_language: Some("en".to_string()),
            task: TranscriptionTask::Transcribe,
            context: RequestContext::default(),
            timeout: TimeoutPolicy::Interactive,
            cancellation: CancellationToken::new(),
            initial_prompt: None,
            audio_ctx: None,
            speed_mode_override: None,
        };

        assert_eq!(request.source, TranscriptionSource::AudioBytes);
        assert_eq!(request.task, TranscriptionTask::Transcribe);
        assert!(!request.cancellation.is_cancelled());
        assert_eq!(request.audio_ctx, None);
        assert_eq!(request.speed_mode_override, None);
    }

    #[test]
    fn from_arc_shares_cancellation_state() {
        let flag = Arc::new(AtomicBool::new(false));
        let token = CancellationToken::from_arc(flag.clone());

        assert!(!token.is_cancelled());

        flag.store(true, Ordering::SeqCst);
        assert!(token.is_cancelled());

        let flag = Arc::new(AtomicBool::new(false));
        let token = CancellationToken::from_arc(flag.clone());

        token.cancel();
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn request_carries_initial_prompt() {
        let request = TranscriptionRequest {
            source: TranscriptionSource::AudioBytes,
            audio: TranscriptionAudio::Bytes {
                bytes: vec![0, 1, 2, 3],
                format_hint: Some(AudioFormatHint::Wav),
            },
            engine: EngineSelection::Explicit {
                engine: ProviderEngine::Whisper,
                model: "base.en".to_string(),
            },
            spoken_language: Some("en".to_string()),
            task: TranscriptionTask::Transcribe,
            context: RequestContext::default(),
            timeout: TimeoutPolicy::Interactive,
            cancellation: CancellationToken::new(),
            initial_prompt: Some("custom vocab".to_string()),
            audio_ctx: None,
            speed_mode_override: None,
        };

        assert_eq!(request.initial_prompt.as_deref(), Some("custom vocab"));
    }

    #[test]
    fn prune_to_leaves_fitting_context_unchanged() {
        let mut context = sample_context();
        let original = context.clone();
        context.prune_to(serialized_len(&context));
        assert_eq!(context, original);
    }

    #[test]
    fn prune_to_drops_trailing_entries_until_context_fits() {
        let mut context = sample_context();
        let mut expected = sample_context();
        expected.corrections.pop();
        expected.terms.pop();
        let cap = serialized_len(&expected);

        context.prune_to(cap);

        assert_eq!(context, expected);
        assert!(serialized_len(&context) <= cap);
    }

    #[test]
    fn prune_to_clears_all_entries_when_empty_context_is_still_over_cap() {
        let mut context = sample_context();
        context.prune_to(1);

        assert!(context.terms.is_empty());
        assert!(context.corrections.is_empty());
    }

    #[test]
    fn prune_to_zero_max_bytes_is_no_op() {
        let mut context = sample_context();
        let original = context.clone();
        context.prune_to(0);
        assert_eq!(context, original);
    }

    #[test]
    fn cancellation_token_defaults_and_clones_share_state() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());

        let clone = token.clone();
        clone.cancel();

        assert!(token.is_cancelled());
        assert!(clone.is_cancelled());
    }
}
