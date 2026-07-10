//! Soniox realtime (WebSocket) STT response mapping — plan 043 (S-MAP + S-ERR).
//!
//! Pure, socket-free logic for the `stt-rt` streaming engine, driven by
//! `SonioxPreviewStreamSink` (C-WS/C-BRIDGE/C-ROUTE). The WS final is the
//! authoritative pasted result (plan 043b); REST-on-WAV is the fallback path.
//! The live WS handshake stays a MANUAL smoke (needs a Soniox key in secure store).
//!
//! Key difference from the REST path (`soniox.rs:230-241`): realtime tokens carry
//! their OWN leading spaces, so committed/tentative text is the VERBATIM
//! concatenation of token `text` fields — never the space-inserting join used
//! for REST. The committed prefix is append-only by construction (finals are
//! emitted once and never reissued by Soniox RT), satisfying
//! `StreamSessionGate::assert_committed_monotonic`.

use serde::Deserialize;

use crate::cloud_stt::common::SttError;

/// One token in a Soniox RT response. RT reissues non-final tokens (they may
/// change/disappear/be replaced) but emits final tokens exactly once. Extra
/// API fields (`speaker`, `start_ms`, `end_ms`, `confidence`, `language`, etc.)
/// are ignored; `text` is required — the only field this mapper consumes.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct SonioxRtToken {
    pub text: String,
    #[serde(default)]
    pub is_final: bool,
}

/// A Soniox RT websocket message. Container-level `#[serde(default)]` makes
/// partial/unknown JSON (e.g. `{}`) deserialize to an all-default value, so a
/// short or malformed frame never panics the folder.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct SonioxRtResponse {
    /// Tokens for this response. Non-final tokens are reissued each response;
    /// final tokens are emitted once.
    pub tokens: Vec<SonioxRtToken>,
    /// Set by the server on the terminal frame after an empty-frame finalize.
    pub finished: bool,
    /// In-band error (HTTP-style code in the message body, not a transport
    /// status) — map via `rt_error_to_stt`.
    pub error_code: Option<i64>,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
}

/// Folded partial result: running committed prefix + this response's tentative tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SonioxRtPartial {
    /// Append-only running text of all `is_final` tokens seen so far.
    pub committed: String,
    /// This response's `is_final:false` tokens, replaced wholesale each frame.
    pub tentative: String,
    /// Monotonic per-response revision (1 after the first ingest; 0 is `Started`).
    pub revision: u64,
}

/// Stateful folder: accumulates Soniox RT token streams into the committed
/// prefix + tentative tail shape expected by `TranscriptionStreamEvent::Partial`.
///
/// `committed` is monotonic-by-construction: each ingest appends this
/// response's `is_final` token `text` VERBATIM (no space insertion), and Soniox
/// RT never re-emits a final token, so every successive `committed` is a strict
/// byte-prefix extension of the previous one.
pub(crate) struct SonioxRtFolder {
    committed: String,
    revision: u64,
}

impl SonioxRtFolder {
    pub(crate) fn new() -> Self {
        Self {
            committed: String::new(),
            revision: 0,
        }
    }

    /// Fold one RT response: append final tokens' `text` to `committed`
    /// verbatim, build `tentative` from non-final tokens, bump the revision.
    pub(crate) fn ingest(&mut self, resp: &SonioxRtResponse) -> SonioxRtPartial {
        let mut tentative = String::new();
        for token in &resp.tokens {
            if token.is_final {
                // VERBATIM append — RT tokens carry their own leading spaces.
                self.committed.push_str(&token.text);
            } else {
                tentative.push_str(&token.text);
            }
        }
        self.revision = self.revision.saturating_add(1);
        SonioxRtPartial {
            committed: self.committed.clone(),
            tentative,
            revision: self.revision,
        }
    }

    /// The current append-only committed prefix (full text of all final tokens).
    pub(crate) fn committed(&self) -> &str {
        &self.committed
    }
}

impl Default for SonioxRtFolder {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a Soniox RT in-band `error_code` (HTTP-style integer in the response
/// body, not a transport status) to an `SttError`, paralleling `classify_status`
/// in `common.rs` (401→Auth, 403/404→ModelUnavailable, 408→Timeout,
/// 429→RateLimited, 5xx→Server, else→BadResponse).
pub(crate) fn rt_error_to_stt(code: i64) -> SttError {
    match code {
        401 => SttError::Auth,
        403 | 404 => SttError::ModelUnavailable,
        408 => SttError::Timeout,
        429 => SttError::RateLimited,
        500..=599 => SttError::Server,
        _ => SttError::BadResponse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription::stream::StreamSessionGate;

    fn token(text: &str, is_final: bool) -> SonioxRtToken {
        SonioxRtToken {
            text: text.to_string(),
            is_final,
        }
    }

    fn response(tokens: Vec<SonioxRtToken>) -> SonioxRtResponse {
        SonioxRtResponse {
            tokens,
            ..Default::default()
        }
    }

    #[test]
    fn finals_accumulate_and_committed_is_append_only_prefix() {
        let mut folder = SonioxRtFolder::new();
        assert_eq!(folder.committed(), "");

        let p1 = folder.ingest(&response(vec![token("Hello", true), token(" world", true)]));
        let p2 = folder.ingest(&response(vec![token(" my", true), token(" friend", true)]));
        let p3 = folder.ingest(&response(vec![token("!", true)]));

        assert_eq!(p1.committed, "Hello world");
        assert_eq!(p2.committed, "Hello world my friend");
        assert_eq!(p3.committed, "Hello world my friend!");

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
    fn tentative_is_replaced_wholesale_each_response() {
        let mut folder = SonioxRtFolder::new();
        let p1 = folder.ingest(&response(vec![token("ab", false), token("c", false)]));
        let p2 = folder.ingest(&response(vec![token("xy", false), token("z", false)]));

        assert_eq!(p1.tentative, "abc");
        // Replaced, not appended: tentative reflects ONLY the current response.
        assert_eq!(p2.tentative, "xyz");
        // No finals seen → committed stays empty across both responses.
        assert_eq!(p1.committed, "");
        assert_eq!(p2.committed, "");
    }

    #[test]
    fn reissued_non_final_tokens_never_corrupt_committed() {
        // A non-final token ("foo") is reissued across responses; it must NEVER
        // land in committed, which only ever takes is_final tokens.
        let mut folder = SonioxRtFolder::new();

        let p1 = folder.ingest(&response(vec![token("foo", false), token("bar", false)]));
        let p2 = folder.ingest(&response(vec![token("foo", false), token("baz", false)]));
        let p3 = folder.ingest(&response(vec![token("real", true), token("foo", false)]));

        assert_eq!(p1.committed, "");
        assert_eq!(p2.committed, ""); // "foo"/"baz" are non-final → ignored
        assert_eq!(p3.committed, "real"); // only the final token contributes
    }

    #[test]
    fn rt_tokens_are_concatenated_verbatim_without_inserted_spaces() {
        // RT tokens carry their OWN leading spaces. Unlike the REST join in
        // soniox.rs (which inserts a space between every token → "world !"),
        // RT concatenation is verbatim: the space before "world" comes from the
        // token, and "!" (no leading space) glues directly onto "world".
        let mut folder = SonioxRtFolder::new();
        let p = folder.ingest(&response(vec![
            token("Hello", true),
            token(" world", true),
            token("!", true),
        ]));
        assert_eq!(p.committed, "Hello world!");

        // A token whose text already has TWO leading spaces keeps BOTH — REST
        // join would collapse to a single space. Contrast proves no insertion.
        let p2 = folder.ingest(&response(vec![token("  indented", true)]));
        assert_eq!(p2.committed, "Hello world!  indented");
        assert!(StreamSessionGate::assert_committed_monotonic(
            &p.committed,
            &p2.committed
        ));
    }

    #[test]
    fn rt_error_to_stt_maps_in_band_codes_like_classify_status() {
        assert!(matches!(rt_error_to_stt(401), SttError::Auth));
        assert!(matches!(rt_error_to_stt(403), SttError::ModelUnavailable));
        assert!(matches!(rt_error_to_stt(404), SttError::ModelUnavailable));
        assert!(matches!(rt_error_to_stt(408), SttError::Timeout));
        assert!(matches!(rt_error_to_stt(429), SttError::RateLimited));
        assert!(matches!(rt_error_to_stt(500), SttError::Server));
        assert!(matches!(rt_error_to_stt(599), SttError::Server));
        assert!(matches!(rt_error_to_stt(0), SttError::BadResponse));
        assert!(matches!(rt_error_to_stt(400), SttError::BadResponse));
        assert!(matches!(rt_error_to_stt(499), SttError::BadResponse));
        assert!(matches!(rt_error_to_stt(600), SttError::BadResponse));
        assert!(matches!(rt_error_to_stt(-1), SttError::BadResponse));
    }

    #[test]
    fn error_response_with_empty_tokens_ingests_cleanly() {
        // An in-band error frame (tokens:[], error_code set) must not panic the
        // folder; committed/tentative stay as-is, the code is surfaced via
        // rt_error_to_stt by the WS task, not the folder.
        let mut folder = SonioxRtFolder::new();
        folder.ingest(&response(vec![token("ok", true)])); // seed committed

        let err_resp = SonioxRtResponse {
            tokens: vec![],
            error_code: Some(429),
            error_type: Some("rate_limited".into()),
            error_message: Some("too many requests".into()),
            ..Default::default()
        };
        let p = folder.ingest(&err_resp);

        assert_eq!(p.committed, "ok"); // unchanged by the error frame
        assert_eq!(p.tentative, "");
        assert!(matches!(
            rt_error_to_stt(err_resp.error_code.unwrap()),
            SttError::RateLimited
        ));
    }

    #[test]
    fn partial_and_garbage_json_deserialize_without_panic() {
        // Rich frame: extra/unknown fields are ignored.
        let rich: SonioxRtResponse = serde_json::from_value(serde_json::json!({
            "tokens": [{
                "text": "hi",
                "is_final": true,
                "speaker": 0,
                "start_ms": 10,
                "end_ms": 40,
                "confidence": 0.99,
                "language": "en"
            }],
            "finished": false,
            "session_id": "abc",
            "unrelated": [1, 2, 3]
        }))
        .expect("rich frame deserializes, extra fields ignored");
        assert_eq!(rich.tokens.len(), 1);
        assert_eq!(rich.tokens[0].text, "hi");
        assert!(rich.tokens[0].is_final);

        // Empty object → all defaults, no panic.
        let empty: SonioxRtResponse =
            serde_json::from_str("{}").expect("empty object deserializes to defaults");
        assert!(empty.tokens.is_empty());
        assert!(!empty.finished);
        assert!(empty.error_code.is_none());

        // Missing is_final defaults to false.
        let no_final: SonioxRtResponse = serde_json::from_value(serde_json::json!({
            "tokens": [{"text": "tentative"}]
        }))
        .unwrap();
        assert!(!no_final.tokens[0].is_final);

        // True garbage yields a default via unwrap_or_default — never a panic.
        let bogus: SonioxRtResponse = serde_json::from_str("not json at all").unwrap_or_default();
        assert!(bogus.tokens.is_empty());
    }
}
