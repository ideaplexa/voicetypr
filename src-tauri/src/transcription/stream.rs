#![allow(dead_code)] // Inert slice for plans/037-stream-event-contract.md.

use serde::{Deserialize, Serialize};

use crate::provider_capabilities::ProviderEngine;

pub const TRANSCRIPTION_STREAM_EVENT: &str = "transcription-stream";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptionStreamEvent {
    Started {
        session_id: u64,
        engine: String,
        revision: u64,
    },
    Partial {
        session_id: u64,
        revision: u64,
        committed: String,
        tentative: String,
    },
    Final {
        session_id: u64,
        revision: u64,
        text: String,
    },
    Cancelled {
        session_id: u64,
        revision: u64,
    },
    Error {
        session_id: u64,
        revision: u64,
        error: String,
    },
}

impl TranscriptionStreamEvent {
    pub fn session_id(&self) -> u64 {
        match self {
            Self::Started { session_id, .. }
            | Self::Partial { session_id, .. }
            | Self::Final { session_id, .. }
            | Self::Cancelled { session_id, .. }
            | Self::Error { session_id, .. } => *session_id,
        }
    }

    pub fn revision(&self) -> u64 {
        match self {
            Self::Started { revision, .. }
            | Self::Partial { revision, .. }
            | Self::Final { revision, .. }
            | Self::Cancelled { revision, .. }
            | Self::Error { revision, .. } => *revision,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineStreamCapabilities {
    pub supports_streaming: bool,
    pub supports_committed_prefix: bool,
    pub supports_tentative_tail: bool,
    pub supports_endpointing: bool,
    pub final_only: bool,
}

impl EngineStreamCapabilities {
    pub const FINAL_ONLY: Self = Self {
        supports_streaming: false,
        supports_committed_prefix: false,
        supports_tentative_tail: false,
        supports_endpointing: false,
        final_only: true,
    };

    // Local Whisper streams via sliding-window decode-ahead (plan 032): a committed
    // prefix + tentative tail, no endpointing (no EOU). No model download required.
    pub const WHISPER: Self = Self {
        supports_streaming: true,
        supports_committed_prefix: true,
        supports_tentative_tail: true,
        supports_endpointing: false,
        final_only: false,
    };
    // Local Parakeet streams via the sidecar decode-ahead engine (plan 051): a
    // committed prefix + tentative tail on a ~1s cadence with pause-aligned commits,
    // no endpointing, no extra model download. Native EOU stays dormant until
    // upstream FluidAudio produces non-empty transcripts again (retested still
    // broken on 0.15.5, 2026-07-10; original evidence plans/042, 2026-07-02).
    pub const PARAKEET: Self = Self {
        supports_streaming: true,
        supports_committed_prefix: true,
        supports_tentative_tail: true,
        supports_endpointing: false,
        final_only: false,
    };
    // Soniox realtime WebSocket (plan 043): committed prefix + tentative tail + native
    // endpoint detection. Result-authoritative (plan 043b): WS-final is the pasted text,
    // REST-on-WAV runs only as fallback. No model download.
    pub const SONIOX: Self = Self {
        supports_streaming: true,
        supports_committed_prefix: true,
        supports_tentative_tail: true,
        supports_endpointing: true,
        final_only: false,
    };
    pub const OPENAI: Self = Self::FINAL_ONLY;
    pub const GROQ: Self = Self::FINAL_ONLY;
    // Deepgram realtime WebSocket (plan 044): committed prefix + tentative tail +
    // native endpointing (speech_final). Result-authoritative: WS-final is the
    // pasted text, REST-on-WAV runs only as fallback. No model download.
    pub const DEEPGRAM: Self = Self {
        supports_streaming: true,
        supports_committed_prefix: true,
        supports_tentative_tail: true,
        supports_endpointing: true,
        final_only: false,
    };
    pub const COHERE: Self = Self::FINAL_ONLY;
    pub const REMOTE: Self = Self::FINAL_ONLY;

    pub fn for_engine(engine: ProviderEngine) -> Self {
        Self::for_platform(engine, std::env::consts::OS)
    }

    pub fn for_platform(engine: ProviderEngine, os: &str) -> Self {
        match engine {
            ProviderEngine::Whisper => Self::WHISPER,
            ProviderEngine::Parakeet if os == "macos" => Self::PARAKEET,
            ProviderEngine::Parakeet => Self::FINAL_ONLY,
            // Soniox realtime streaming is result-authoritative (plan 043b): the WS
            // final is the pasted text; REST-on-WAV runs only as fallback.
            ProviderEngine::Soniox => Self::SONIOX,
            ProviderEngine::Openai => Self::OPENAI,
            ProviderEngine::Groq => Self::GROQ,
            ProviderEngine::Deepgram => Self::DEEPGRAM,
            ProviderEngine::Cohere => Self::COHERE,
            ProviderEngine::Remote => Self::REMOTE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    Accept,
    StaleSession,
    StaleRevision,
    /// A terminal `Final`/`Cancelled` was already admitted; nothing may follow.
    /// Closes the residual race where a preview decode still in flight at stop emits
    /// a late same-session `Partial` after the pill has shown its final state.
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamSessionGate {
    active_session_id: u64,
    last_revision: Option<u64>,
    terminal: bool,
}

impl StreamSessionGate {
    pub fn new(active_session_id: u64) -> Self {
        Self {
            active_session_id,
            last_revision: None,
            terminal: false,
        }
    }

    pub fn from_current_recording_generation() -> Self {
        Self::new(crate::commands::audio::current_recording_generation())
    }

    pub fn active_session_id(&self) -> u64 {
        self.active_session_id
    }

    pub fn last_revision(&self) -> Option<u64> {
        self.last_revision
    }

    pub fn admit(&mut self, event: &TranscriptionStreamEvent) -> Admit {
        // Once a terminal event is admitted the session is closed: reject everything
        // after it (a late in-flight preview `Partial` must never land post-stop).
        if self.terminal {
            return Admit::Closed;
        }
        if event.session_id() != self.active_session_id {
            return Admit::StaleSession;
        }

        let revision = event.revision();
        if self
            .last_revision
            .is_some_and(|last_revision| revision <= last_revision)
        {
            return Admit::StaleRevision;
        }

        self.last_revision = Some(revision);
        // Final / Cancelled / Error all end the session — nothing may follow (a late
        // in-flight preview Partial must never land after any terminal event).
        if matches!(
            event,
            TranscriptionStreamEvent::Final { .. }
                | TranscriptionStreamEvent::Cancelled { .. }
                | TranscriptionStreamEvent::Error { .. }
        ) {
            self.terminal = true;
        }
        Admit::Accept
    }

    pub fn assert_committed_monotonic(prev_committed: &str, next_committed: &str) -> bool {
        next_committed
            .get(..prev_committed.len())
            .is_some_and(|prefix| prefix == prev_committed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::audio::begin_recording_generation;
    use serde_json::json;

    fn final_event(session_id: u64, revision: u64) -> TranscriptionStreamEvent {
        TranscriptionStreamEvent::Final {
            session_id,
            revision,
            text: "done".to_string(),
        }
    }

    fn partial_event(session_id: u64, revision: u64) -> TranscriptionStreamEvent {
        TranscriptionStreamEvent::Partial {
            session_id,
            revision,
            committed: String::new(),
            tentative: String::new(),
        }
    }

    fn error_event(session_id: u64, revision: u64) -> TranscriptionStreamEvent {
        TranscriptionStreamEvent::Error {
            session_id,
            revision,
            error: "boom".to_string(),
        }
    }

    #[test]
    fn stale_session_is_dropped() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let stale_session = begin_recording_generation();
        let current_session = begin_recording_generation();
        let mut gate = StreamSessionGate::from_current_recording_generation();

        assert_eq!(gate.active_session_id(), current_session);
        assert_eq!(
            gate.admit(&final_event(stale_session, 1)),
            Admit::StaleSession
        );
        assert_eq!(gate.last_revision(), None);
    }

    #[test]
    fn revision_ordering_rejects_stale_and_allows_forward_gaps() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let session_id = begin_recording_generation();
        let mut gate = StreamSessionGate::new(session_id);

        // Non-terminal Partials so the gate stays open across the revision checks.
        assert_eq!(gate.admit(&partial_event(session_id, 3)), Admit::Accept);
        assert_eq!(gate.last_revision(), Some(3));
        assert_eq!(
            gate.admit(&partial_event(session_id, 3)),
            Admit::StaleRevision
        );
        assert_eq!(
            gate.admit(&partial_event(session_id, 2)),
            Admit::StaleRevision
        );
        assert_eq!(gate.admit(&partial_event(session_id, 8)), Admit::Accept);
        assert_eq!(gate.last_revision(), Some(8));
    }

    #[test]
    fn terminal_event_closes_the_gate_against_late_partials() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let session_id = begin_recording_generation();
        let mut gate = StreamSessionGate::new(session_id);

        assert_eq!(gate.admit(&partial_event(session_id, 1)), Admit::Accept);
        // A terminal Final closes the session.
        assert_eq!(gate.admit(&final_event(session_id, 2)), Admit::Accept);
        // Any later event — even a same-session, higher-revision Partial from a preview
        // decode still in flight at stop — is now rejected as Closed.
        assert_eq!(gate.admit(&partial_event(session_id, 3)), Admit::Closed);
        assert_eq!(gate.admit(&final_event(session_id, 4)), Admit::Closed);
    }

    #[test]
    fn error_event_is_terminal_and_closes_the_gate() {
        let _lifecycle_guard = crate::tests::RECORDING_LIFECYCLE_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let session_id = begin_recording_generation();
        let mut gate = StreamSessionGate::new(session_id);

        assert_eq!(gate.admit(&partial_event(session_id, 1)), Admit::Accept);
        // An Error ends the session (executor falls back to the batch/REST result).
        assert_eq!(gate.admit(&error_event(session_id, 2)), Admit::Accept);
        assert_eq!(gate.admit(&partial_event(session_id, 3)), Admit::Closed);
    }

    #[test]
    fn committed_monotonicity_accepts_growth_and_rejects_shrink_or_rewrite() {
        assert!(StreamSessionGate::assert_committed_monotonic("", "hello"));
        assert!(StreamSessionGate::assert_committed_monotonic(
            "hello",
            "hello world"
        ));
        assert!(StreamSessionGate::assert_committed_monotonic("é", "éclair"));
        assert!(StreamSessionGate::assert_committed_monotonic(
            "日本",
            "日本語"
        ));
        assert!(StreamSessionGate::assert_committed_monotonic(
            "hello 😀",
            "hello 😀!"
        ));

        assert!(!StreamSessionGate::assert_committed_monotonic(
            "hello", "hell"
        ));
        assert!(!StreamSessionGate::assert_committed_monotonic(
            "hello", "hullo"
        ));
        assert!(!StreamSessionGate::assert_committed_monotonic("é", "e"));
        assert!(!StreamSessionGate::assert_committed_monotonic(
            "日本語",
            "日本"
        ));
        assert!(!StreamSessionGate::assert_committed_monotonic("😀", "😃"));
    }

    #[test]
    fn committed_monotonicity_rejects_non_char_boundary_prefix_lengths() {
        assert!(!StreamSessionGate::assert_committed_monotonic("é", "é"));
        assert!(!StreamSessionGate::assert_committed_monotonic("😀", "a😀"));
    }

    #[test]
    fn capability_shape_for_every_current_engine() {
        assert_eq!(
            EngineStreamCapabilities::for_platform(ProviderEngine::Parakeet, "windows"),
            EngineStreamCapabilities::FINAL_ONLY,
        );
        assert_eq!(
            EngineStreamCapabilities::for_platform(ProviderEngine::Parakeet, "macos"),
            EngineStreamCapabilities::PARAKEET,
        );
        // Whisper streams via decode-ahead (plan 032, no endpointing); Parakeet via
        // the sidecar decode-ahead engine (plan 051, same shape). Soniox realtime
        // streaming (plan 043) is result-authoritative (plan 043b): WS-final is the
        // pasted text; REST-on-WAV runs only as fallback. Deepgram realtime streaming
        // (plan 044) mirrors Soniox's stance. The rest are final-only.
        for local_decode_ahead in [ProviderEngine::Whisper, ProviderEngine::Parakeet] {
            if local_decode_ahead == ProviderEngine::Parakeet && !cfg!(target_os = "macos") {
                continue;
            }
            assert_eq!(
                EngineStreamCapabilities::for_engine(local_decode_ahead),
                EngineStreamCapabilities {
                    supports_streaming: true,
                    supports_committed_prefix: true,
                    supports_tentative_tail: true,
                    supports_endpointing: false,
                    final_only: false,
                },
                "{local_decode_ahead:?}"
            );
        }
        assert_eq!(
            EngineStreamCapabilities::for_engine(ProviderEngine::Soniox),
            EngineStreamCapabilities {
                supports_streaming: true,
                supports_committed_prefix: true,
                supports_tentative_tail: true,
                supports_endpointing: true,
                final_only: false,
            },
        );
        assert_eq!(
            EngineStreamCapabilities::for_engine(ProviderEngine::Deepgram),
            EngineStreamCapabilities {
                supports_streaming: true,
                supports_committed_prefix: true,
                supports_tentative_tail: true,
                supports_endpointing: true,
                final_only: false,
            },
        );

        let final_only_engines = [
            ProviderEngine::Openai,
            ProviderEngine::Groq,
            ProviderEngine::Cohere,
            ProviderEngine::Remote,
        ];
        for engine in final_only_engines {
            assert_eq!(
                EngineStreamCapabilities::for_engine(engine),
                EngineStreamCapabilities::FINAL_ONLY,
                "{engine:?}"
            );
        }
    }

    #[test]
    fn serde_round_trips_started_with_snake_case_tag() {
        let event = TranscriptionStreamEvent::Started {
            session_id: 7,
            engine: "whisper".to_string(),
            revision: 0,
        };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "started",
                "session_id": 7,
                "engine": "whisper",
                "revision": 0
            })
        );
        assert_eq!(
            serde_json::from_value::<TranscriptionStreamEvent>(value).unwrap(),
            event
        );
    }

    #[test]
    fn serde_round_trips_partial_with_snake_case_tag() {
        let event = TranscriptionStreamEvent::Partial {
            session_id: 7,
            revision: 1,
            committed: "hello ".to_string(),
            tentative: "wor".to_string(),
        };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "partial",
                "session_id": 7,
                "revision": 1,
                "committed": "hello ",
                "tentative": "wor"
            })
        );
        assert_eq!(
            serde_json::from_value::<TranscriptionStreamEvent>(value).unwrap(),
            event
        );
    }

    #[test]
    fn serde_round_trips_final_with_snake_case_tag() {
        let event = TranscriptionStreamEvent::Final {
            session_id: 7,
            revision: 2,
            text: "hello world".to_string(),
        };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "final",
                "session_id": 7,
                "revision": 2,
                "text": "hello world"
            })
        );
        assert_eq!(
            serde_json::from_value::<TranscriptionStreamEvent>(value).unwrap(),
            event
        );
    }

    #[test]
    fn serde_round_trips_cancelled_with_snake_case_tag() {
        let event = TranscriptionStreamEvent::Cancelled {
            session_id: 7,
            revision: 3,
        };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "cancelled",
                "session_id": 7,
                "revision": 3
            })
        );
        assert_eq!(
            serde_json::from_value::<TranscriptionStreamEvent>(value).unwrap(),
            event
        );
    }

    #[test]
    fn serde_round_trips_error_with_snake_case_tag() {
        let event = TranscriptionStreamEvent::Error {
            session_id: 7,
            revision: 4,
            error: "transcription failed".to_string(),
        };

        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "error",
                "session_id": 7,
                "revision": 4,
                "error": "transcription failed"
            })
        );
        assert_eq!(
            serde_json::from_value::<TranscriptionStreamEvent>(value).unwrap(),
            event
        );
    }
}
