//! Deepgram realtime (WebSocket) STT response mapping — plan 044.
//!
//! Pure, socket-free logic for the `deepgram` streaming engine, driven by
//! `DeepgramPreviewStreamSink`. The WS final is the authoritative pasted result
//! (same stance as plan 043b for Soniox); REST-on-WAV is the fallback path. The
//! live WS handshake stays a MANUAL smoke (needs a Deepgram key in secure store).
//!
//! Key difference from Soniox RT: Deepgram Results messages carry whole-window
//! transcripts (NOT tokens with leading spaces). Interim results (`is_final:false`)
//! cover the CURRENT window and are replaced wholesale each message; `is_final:true`
//! finalizes that window's transcript EXACTLY ONCE (never re-issued). The folder
//! joins finals with a single space separator (no leading space on the first).
//! Empty/whitespace-only finals are skipped to avoid injecting bare spaces for
//! silence windows. The committed prefix is append-only by construction (finals
//! are emitted once), satisfying `StreamSessionGate::assert_committed_monotonic`.

use serde::Deserialize;

/// One alternative in a Deepgram RT response's channel. `transcript` is the only
/// field consumed; extra API fields (`confidence`, `words`, etc.) are ignored.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct DeepgramRtAlternative {
    pub transcript: String,
}

/// The `channel` object in a Deepgram RT Results message.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct DeepgramRtChannel {
    pub alternatives: Vec<DeepgramRtAlternative>,
}

impl DeepgramRtChannel {
    /// The best alternative's transcript, or empty when `alternatives` is absent.
    fn transcript(&self) -> &str {
        self.alternatives
            .first()
            .map(|a| a.transcript.as_str())
            .unwrap_or("")
    }
}

/// A Deepgram RT websocket message. Container-level `#[serde(default)]` makes
/// partial/unknown JSON (e.g. `{}`) deserialize to an all-default value, so a
/// short or malformed frame never panics the folder.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct DeepgramRtResponse {
    pub r#type: String,
    pub is_final: bool,
    pub speech_final: bool,
    pub channel: DeepgramRtChannel,
}

/// Folded partial result: running committed prefix + this response's tentative tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeepgramRtPartial {
    /// Append-only running text of all `is_final:true` transcripts, space-joined.
    pub committed: String,
    /// The current window's interim (`is_final:false`) transcript, replaced wholesale.
    pub tentative: String,
    /// Monotonic per-response revision (1 after the first ingest; 0 is `Started`).
    pub revision: u64,
}

/// Stateful folder: accumulates Deepgram RT Results messages into the committed
/// prefix + tentative tail shape expected by `TranscriptionStreamEvent::Partial`.
///
/// `committed` is monotonic-by-construction: each ingest appends this response's
/// `is_final:true` transcript (space-joined), and Deepgram RT never re-emits a
/// final result, so every successive `committed` is a byte-prefix extension of the
/// previous one. Only `type == "Results"` mutates state; other message types
/// (`Metadata`, unknown) bump the revision but leave committed/tentative unchanged.
pub(crate) struct DeepgramRtFolder {
    committed: String,
    revision: u64,
}

impl DeepgramRtFolder {
    pub(crate) fn new() -> Self {
        Self {
            committed: String::new(),
            revision: 0,
        }
    }

    /// Fold one RT response.
    ///
    /// Only `type == "Results"` mutates state; other types (`Metadata`, unknown)
    /// bump the revision and return the current state unchanged (harmless no-ops).
    /// `is_final:true` with a non-empty (after trim) transcript → append to
    /// `committed` with a single `' '` separator (no leading space on the first);
    /// EMPTY finals are skipped (silence windows must not inject bare spaces).
    /// `is_final:false` → `tentative` = that transcript, replaced wholesale. A final
    /// Results message CLEARS tentative (its window just committed).
    pub(crate) fn ingest(&mut self, resp: &DeepgramRtResponse) -> DeepgramRtPartial {
        let mut tentative = String::new();

        if resp.r#type == "Results" {
            let trimmed = resp.channel.transcript().trim();
            if resp.is_final {
                if !trimmed.is_empty() {
                    // Space-join finals: no separator before the first.
                    if !self.committed.is_empty() {
                        self.committed.push(' ');
                    }
                    self.committed.push_str(trimmed);
                }
                // A final Results message clears tentative: its window just
                // committed, so the previous interim tail is no longer valid.
            } else {
                tentative.push_str(trimmed);
            }
        }

        self.revision = self.revision.saturating_add(1);
        DeepgramRtPartial {
            committed: self.committed.clone(),
            tentative,
            revision: self.revision,
        }
    }

    /// The current append-only committed prefix.
    pub(crate) fn committed(&self) -> &str {
        &self.committed
    }
}

impl Default for DeepgramRtFolder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::stream::StreamSessionGate;

    fn alt(transcript: &str) -> DeepgramRtAlternative {
        DeepgramRtAlternative {
            transcript: transcript.to_string(),
        }
    }

    fn results(transcript: &str, is_final: bool) -> DeepgramRtResponse {
        DeepgramRtResponse {
            r#type: "Results".to_string(),
            is_final,
            speech_final: false,
            channel: DeepgramRtChannel {
                alternatives: vec![alt(transcript)],
            },
        }
    }

    #[test]
    fn finals_accumulate_space_joined_and_committed_is_append_only_prefix() {
        let mut folder = DeepgramRtFolder::new();
        assert_eq!(folder.committed(), "");

        let p1 = folder.ingest(&results("Hello", true));
        let p2 = folder.ingest(&results("world", true));
        let p3 = folder.ingest(&results("!", true));

        assert_eq!(p1.committed, "Hello");
        assert_eq!(p2.committed, "Hello world");
        assert_eq!(p3.committed, "Hello world !");

        // Monotonic-by-construction: every committed is a byte prefix of the next.
        assert!(StreamSessionGate::assert_committed_monotonic(
            &p1.committed,
            &p2.committed
        ));
        assert!(StreamSessionGate::assert_committed_monotonic(
            &p2.committed,
            &p3.committed
        ));

        // Revisions are strictly monotonic per-response.
        assert_eq!(p1.revision, 1);
        assert_eq!(p2.revision, 2);
        assert_eq!(p3.revision, 3);
    }

    #[test]
    fn empty_or_whitespace_finals_are_skipped() {
        let mut folder = DeepgramRtFolder::new();
        let p1 = folder.ingest(&results("Hello", true));
        let p2 = folder.ingest(&results("   ", true)); // whitespace-only — skipped
        let p3 = folder.ingest(&results("", true)); // empty — skipped
        let p4 = folder.ingest(&results("world", true));

        assert_eq!(p1.committed, "Hello");
        // Empty/whitespace finals must not inject bare spaces.
        assert_eq!(p2.committed, "Hello");
        assert_eq!(p3.committed, "Hello");
        assert_eq!(p4.committed, "Hello world");
    }

    #[test]
    fn tentative_is_replaced_wholesale_and_cleared_by_final() {
        let mut folder = DeepgramRtFolder::new();

        let p1 = folder.ingest(&results("hel", false));
        assert_eq!(p1.tentative, "hel");
        assert_eq!(p1.committed, "");

        let p2 = folder.ingest(&results("hello", false));
        // Replaced, not appended: tentative reflects ONLY the current window.
        assert_eq!(p2.tentative, "hello");
        assert_eq!(p2.committed, "");

        let p3 = folder.ingest(&results("hello", true));
        // Final clears tentative (its window just committed).
        assert_eq!(p3.tentative, "");
        assert_eq!(p3.committed, "hello");
    }

    #[test]
    fn metadata_and_unknown_types_dont_corrupt_state() {
        let mut folder = DeepgramRtFolder::new();
        folder.ingest(&results("seed", true));

        // Metadata message — must not corrupt state.
        let metadata = DeepgramRtResponse {
            r#type: "Metadata".to_string(),
            ..Default::default()
        };
        let p = folder.ingest(&metadata);
        assert_eq!(p.committed, "seed"); // unchanged
        assert_eq!(p.tentative, ""); // unchanged
        assert_eq!(p.revision, 2); // revision still bumps

        // Unknown type with a non-empty transcript — also a harmless no-op
        // (only type == "Results" mutates state).
        let unknown = DeepgramRtResponse {
            r#type: "SomeFutureType".to_string(),
            is_final: true,
            channel: DeepgramRtChannel {
                alternatives: vec![alt("ignored")],
            },
            ..Default::default()
        };
        let p2 = folder.ingest(&unknown);
        assert_eq!(p2.committed, "seed"); // unchanged
        assert_eq!(p2.revision, 3);
    }

    #[test]
    fn partial_and_garbage_json_deserialize_without_panic() {
        // Rich frame: extra/unknown fields are ignored.
        let rich: DeepgramRtResponse = serde_json::from_value(serde_json::json!({
            "type": "Results",
            "is_final": true,
            "speech_final": true,
            "channel": {
                "alternatives": [{"transcript": "hi", "confidence": 0.99, "words": []}]
            },
            "channel_index": [0, 1],
            "duration": 1.5,
        }))
        .expect("rich frame deserializes, extra fields ignored");
        assert_eq!(rich.channel.transcript(), "hi");
        assert!(rich.is_final);

        // Empty object → all defaults, no panic.
        let empty: DeepgramRtResponse =
            serde_json::from_str("{}").expect("empty object deserializes to defaults");
        assert!(!empty.is_final);
        assert_eq!(empty.channel.transcript(), "");

        // Missing alternatives → empty transcript via default.
        let no_alts: DeepgramRtResponse = serde_json::from_value(serde_json::json!({
            "type": "Results",
            "is_final": false,
            "channel": {}
        }))
        .unwrap();
        assert_eq!(no_alts.channel.transcript(), "");

        // True garbage yields a default via unwrap_or_default — never a panic.
        let bogus: DeepgramRtResponse = serde_json::from_str("not json at all").unwrap_or_default();
        assert_eq!(bogus.channel.transcript(), "");
    }
}
