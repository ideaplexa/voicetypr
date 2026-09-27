use super::agent_cli::AgentCliRuntime;
use super::contract::{AiPolishRequest, AiPolishResult};
use super::error::{AiProviderError, MappedAiProviderError};
use super::genai_runtime::{AiKeyResolver, GenaiRuntime};
use super::openai_compatible::OpenAiCompatibleRuntime;
use super::providers::PROVIDER_CUSTOM;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct AiExecutor {
    genai_runtime: GenaiRuntime,
    openai_compatible_runtime: OpenAiCompatibleRuntime,
    agent_cli_runtime: AgentCliRuntime,
}

#[derive(Clone)]
pub struct OpenAiCompatibleConfig {
    pub base_url: String,
    pub no_auth: bool,
    pub key_provider_id: String,
    pub extra_headers: Vec<(String, String)>,
}

impl OpenAiCompatibleConfig {
    pub fn custom(base_url: String, no_auth: bool) -> Self {
        Self {
            base_url,
            no_auth,
            key_provider_id: PROVIDER_CUSTOM.to_string(),
            extra_headers: Vec::new(),
        }
    }
}

impl AiExecutor {
    pub fn new(
        http_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        custom_base_url: String,
        custom_no_auth: bool,
    ) -> Self {
        Self::with_native_endpoint_overrides(
            http_client,
            key_resolver,
            OpenAiCompatibleConfig::custom(custom_base_url, custom_no_auth),
            HashMap::new(),
        )
    }

    pub fn with_native_endpoint_overrides(
        http_client: reqwest::Client,
        key_resolver: AiKeyResolver,
        openai_compatible_config: OpenAiCompatibleConfig,
        native_endpoint_overrides: HashMap<String, String>,
    ) -> Self {
        Self {
            genai_runtime: GenaiRuntime::with_endpoint_overrides(
                http_client.clone(),
                key_resolver.clone(),
                native_endpoint_overrides,
            ),
            openai_compatible_runtime: OpenAiCompatibleRuntime::new(
                http_client,
                key_resolver,
                openai_compatible_config.base_url,
                openai_compatible_config.no_auth,
                openai_compatible_config.key_provider_id,
                openai_compatible_config.extra_headers,
            ),
            agent_cli_runtime: AgentCliRuntime::new(),
        }
    }

    pub async fn polish(
        &self,
        request: AiPolishRequest,
        cancellation_token: CancellationToken,
    ) -> Result<AiPolishResult, AiProviderError> {
        let start = Instant::now();
        let budget = Duration::from_millis(request.timeout_ms);
        let deadline = start + budget;
        let mut attempt = 0_u8;

        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AiProviderError::Timeout)?;
            let result = self
                .run_with_budget(&request, cancellation_token.clone(), remaining)
                .await;

            match result {
                Ok(output_text) => {
                    let (cleaned, truncated) =
                        sanitize_ai_output(&output_text, request.input_text.len());
                    let validation = if truncated {
                        Err(AiProviderError::BadResponse)
                    } else {
                        validate_ai_output(&cleaned, &request.input_text)
                    };
                    let validated = match validation {
                        Ok(output) => output,
                        Err(error) if attempt == 0 => {
                            attempt += 1;
                            log::warn!(
                                "AI cleanup response failed validation; retrying once category={:?}",
                                error
                            );
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    if validated.trim().is_empty() {
                        return Err(AiProviderError::BadResponse);
                    }
                    return Ok(AiPolishResult {
                        output_text: validated,
                        provider_id: request.provider_id,
                        model_id: request.model_id,
                        duration_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                    });
                }
                Err(mapped) if attempt == 0 && should_retry(&mapped.error) => {
                    attempt += 1;
                    if let Some(retry_after) = mapped.retry_after {
                        let remaining_after_sleep = deadline
                            .checked_duration_since(Instant::now())
                            .ok_or(AiProviderError::Timeout)?;
                        if retry_after >= remaining_after_sleep {
                            return Err(mapped.error);
                        }
                        tokio::select! {
                            _ = cancellation_token.cancelled() => return Err(AiProviderError::Canceled),
                            _ = tokio::time::sleep(retry_after) => {}
                        }
                    }
                }
                Err(mapped) => return Err(mapped.error),
            }
        }
    }

    async fn run_with_budget(
        &self,
        request: &AiPolishRequest,
        cancellation_token: CancellationToken,
        remaining: Duration,
    ) -> Result<String, MappedAiProviderError> {
        tokio::select! {
            biased;
            _ = tokio::time::sleep(remaining) => Err(MappedAiProviderError::new(AiProviderError::Timeout)),
            _ = cancellation_token.cancelled() => Err(MappedAiProviderError::new(AiProviderError::Canceled)),
            result = self.execute_once(request) => result,
        }
    }

    async fn execute_once(
        &self,
        request: &AiPolishRequest,
    ) -> Result<String, MappedAiProviderError> {
        if crate::ai::catalog::is_native_provider(&request.provider_id) {
            self.genai_runtime.polish(request).await
        } else if crate::ai::catalog::runtime_kind(&request.provider_id)
            == Some("openai_compatible")
        {
            self.openai_compatible_runtime.polish(request).await
        } else if crate::ai::catalog::runtime_kind(&request.provider_id) == Some("agent_cli") {
            self.agent_cli_runtime.polish(request).await
        } else {
            Err(MappedAiProviderError::new(
                AiProviderError::UnsupportedProvider,
            ))
        }
    }
}

fn should_retry(error: &AiProviderError) -> bool {
    matches!(
        error,
        AiProviderError::RateLimited
            | AiProviderError::ServiceUnavailable
            | AiProviderError::Network
    )
}

fn validate_ai_output(output: &str, input: &str) -> Result<String, AiProviderError> {
    let cleaned = strip_wrapping_quotes(
        strip_known_preamble(
            strip_wrapping_quotes(strip_markdown_fence(output).trim(), input),
            input,
        ),
        input,
    )
    .trim()
    .to_string();

    if cleaned.is_empty()
        || starts_with_refusal_or_commentary(&cleaned)
        || has_anomalous_cleanup_length(&cleaned, input)
    {
        Err(AiProviderError::BadResponse)
    } else {
        Ok(cleaned)
    }
}

fn strip_markdown_fence(output: &str) -> &str {
    let trimmed = output.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }

    let Some(first_newline) = trimmed.find('\n') else {
        return trimmed;
    };
    let body_and_close = &trimmed[first_newline + 1..];
    let Some(close_start) = body_and_close.rfind("```") else {
        return trimmed;
    };
    if body_and_close[close_start + 3..].trim().is_empty() {
        body_and_close[..close_start].trim()
    } else {
        trimmed
    }
}

fn strip_wrapping_quotes<'a>(output: &'a str, input: &str) -> &'a str {
    let trimmed = output.trim();
    let input_trimmed = input.trim();
    if is_wrapped_in_quotes(input_trimmed).is_some() {
        return trimmed;
    }

    match is_wrapped_in_quotes(trimmed) {
        Some((open_len, close_len)) => trimmed[open_len..trimmed.len() - close_len].trim(),
        None => trimmed,
    }
}

fn is_wrapped_in_quotes(text: &str) -> Option<(usize, usize)> {
    let pairs = [
        ('"', '"'),
        ('\'', '\''),
        ('\u{201c}', '\u{201d}'),
        ('\u{2018}', '\u{2019}'),
    ];
    pairs.iter().find_map(|(open, close)| {
        if text.starts_with(*open) && text.ends_with(*close) && text.len() > open.len_utf8() {
            Some((open.len_utf8(), close.len_utf8()))
        } else {
            None
        }
    })
}

/// Normalize a candidate first line for preamble handling: trim whitespace,
/// strip wrapping quotes, drop trailing colons. Case-insensitive comparison
/// is applied by the caller. Shared by the ownership guard and the known-list
/// lookup so both agree on what "the same line" means.
fn normalize_preamble_line(line: &str) -> &str {
    line.trim()
        .trim_matches(|ch: char| ch == '"' || ch == '\'' || ch.is_whitespace())
        .trim_end_matches(':')
        .trim()
}

fn strip_known_preamble<'a>(output: &'a str, input: &str) -> &'a str {
    // Single-line output has no preamble; the common case returns before any
    // guard work.
    let Some((first_line, rest)) = output.split_once('\n') else {
        return output;
    };
    // The first line is only a strip candidate if the input does not itself
    // begin with that line. When it does, the line is user content even if
    // the rest was polished ("Sure\nI will send it." -> "Sure:\nI'll send
    // it."): stripping it would delete the user's words. Both sides go
    // through the same borrowed normalizer as the known-list lookup below
    // (whitespace, wrapping quotes, trailing colons, then ASCII
    // case-insensitive compare), so a punctuation-fiddled echo still counts
    // as user content.
    let candidate = normalize_preamble_line(first_line);
    let input_first_line = normalize_preamble_line(input.split('\n').next().unwrap_or(input));
    if !candidate.is_empty() && candidate.eq_ignore_ascii_case(input_first_line) {
        return output;
    }
    // NOTE: no bare "sure" entry. A standalone greeting/acknowledgement is
    // plausibly the user's own dictated content ("Sure!\nI will send it
    // tomorrow.", or reflowed to "Sure\nI'll send it tomorrow."), so
    // stripping it deletes user words. Only an explicit "Here is the …"
    // wrapper phrase counts as a removable preamble.
    let known = [
        "here is the fixed text",
        "here's the fixed text",
        "here is the cleaned text",
        "here's the cleaned text",
        "here is the corrected text",
        "here's the corrected text",
        "here is the polished text",
        "here's the polished text",
        "sure, here is the fixed text",
        "sure, here's the fixed text",
        "sure, here is the cleaned text",
        "sure, here's the cleaned text",
        "sure, here is the corrected text",
        "sure, here's the corrected text",
        "sure, here is the polished text",
        "sure, here's the polished text",
    ];
    if known
        .iter()
        .any(|entry| entry.eq_ignore_ascii_case(candidate))
    {
        rest.trim()
    } else {
        output
    }
}

fn starts_with_refusal_or_commentary(output: &str) -> bool {
    let lower = output.trim_start().to_ascii_lowercase();
    lower.starts_with("i can't")
        || lower.starts_with("i cannot")
        || lower.starts_with("i'm sorry")
        || lower.starts_with("i am sorry")
}

fn has_anomalous_cleanup_length(output: &str, input: &str) -> bool {
    let input_len = input.trim().len();
    let output_len = output.trim().len();
    if input_len < 80 {
        output_len > 4096
    } else if input_len >= 256 {
        output_len >= input_len.saturating_mul(4).saturating_sub(32)
    } else {
        output_len > input_len.saturating_mul(12).max(4096)
    }
}

/// Sanitize a model's cleanup response before it is returned for auto-typing.
///
/// Drops control characters except `\n` and `\t` (carriage returns collapse to
/// `\n`) and the bidirectional-formatting controls (Unicode `Cf`) used in
/// Trojan-Source-style injection, then enforces a length ceiling relative to
/// the input so a runaway model cannot dump unbounded text at the cursor.
fn sanitize_ai_output(output: &str, input_byte_len: usize) -> (String, bool) {
    // 4x covers normal cleanup/translation; the floor keeps short inputs (whose
    // cleaned form can be several times larger) from being clipped.
    let cap = input_byte_len
        .saturating_mul(4)
        .max(usize::try_from(super::contract::AI_OUTPUT_MIN_TOKEN_CAP).unwrap_or(4096) * 4);

    let mut sanitized = String::with_capacity(output.len().min(cap));
    let mut chars = output.chars().peekable();
    let mut truncated = false;
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if sanitized.len() + '\n'.len_utf8() > cap {
                truncated = true;
                break;
            }
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            sanitized.push('\n');
        } else if ch == '\n' || ch == '\t' {
            if sanitized.len() + ch.len_utf8() > cap {
                truncated = true;
                break;
            }
            sanitized.push(ch);
        } else if ch.is_control() || is_bidi_override(ch) {
            // Drop Cc control and Cf bidi-format characters.
        } else {
            if sanitized.len() + ch.len_utf8() > cap {
                truncated = true;
                break;
            }
            sanitized.push(ch);
        }
    }
    if truncated {
        log::warn!(
            "AI cleanup output exceeded the {cap}-byte length ceiling; truncated before insertion"
        );
    }
    (sanitized, truncated)
}

/// Bidirectional-formatting controls (Unicode category `Cf`) with no legitimate
/// place in auto-typed prose: the Trojan-Source override set. Zero-width
/// joiners/non-joiners are intentionally excluded — they are load-bearing in
/// some scripts and emoji sequences, so stripping them would corrupt text.
fn is_bidi_override(ch: char) -> bool {
    matches!(
        ch,
        '\u{061C}'                          // ARABIC LETTER MARK
            | '\u{200E}' | '\u{200F}'       // LTR / RTL MARK
            | '\u{202A}'..='\u{202E}'       // LRE / RLE / PDF / LRO / RLO
            | '\u{2066}'..='\u{2069}'       // LRI / RLI / FSI / PDI
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_strips_known_preamble_followed_by_newline() {
        let output = "Here is the fixed text:\nMeet me at 4.";
        assert_eq!(
            validate_ai_output(output, "meet me at four").unwrap(),
            "Meet me at 4."
        );
    }

    #[test]
    fn validate_strips_markdown_fence() {
        let output = "```text\nMeet me at 4.\n```";
        assert_eq!(
            validate_ai_output(output, "meet me at four").unwrap(),
            "Meet me at 4."
        );
    }

    #[test]
    fn validate_strips_wrapping_quotes_when_input_was_not_quoted() {
        let output = "\"Meet me at 4.\"";
        assert_eq!(
            validate_ai_output(output, "meet me at four").unwrap(),
            "Meet me at 4."
        );
    }

    #[test]
    fn validate_preserves_wrapping_quotes_when_input_was_quoted() {
        let output = "\"Meet me at 4.\"";
        assert_eq!(
            validate_ai_output(output, "\"meet me at four\"").unwrap(),
            "\"Meet me at 4.\""
        );
    }

    #[test]
    fn validate_rejects_refusal_commentary() {
        let error = validate_ai_output("I'm sorry, I can't do that.", "hello").unwrap_err();
        assert!(matches!(error, AiProviderError::BadResponse));
    }

    #[test]
    fn validate_keeps_identity_output_unchanged() {
        let output = "Already clean.";
        assert_eq!(validate_ai_output(output, output).unwrap(), output);
    }

    #[test]
    fn validate_preserves_identity_output_whose_first_line_looks_like_a_preamble() {
        // Regression: an identity echo whose first line matches a known
        // preamble ("Sure") used to have that line deleted — the model
        // repeated the input verbatim, so "Sure" is user content, not a
        // model wrapper.
        let input = "Sure\nI will send it tomorrow.";
        assert_eq!(validate_ai_output(input, input).unwrap(), input);
    }

    #[test]
    fn validate_preserves_identity_echo_delivered_in_a_markdown_fence() {
        let input = "Sure\nI will send it tomorrow.";
        let output = "```\nSure\nI will send it tomorrow.\n```";
        assert_eq!(validate_ai_output(output, input).unwrap(), input);
    }

    #[test]
    fn validate_preserves_standalone_sure_despite_punctuation_variant() {
        // Regression: input "Sure!\n…" echoed with different punctuation
        // ("Sure:") used to normalize to the (since removed) bare "sure"
        // preamble entry and delete the user's first line.
        let input = "Sure!\nI will send it tomorrow.";
        let output = "Sure:\nI\u{2019}ll send it tomorrow.";
        assert_eq!(validate_ai_output(output, input).unwrap(), output);
    }

    #[test]
    fn validate_preserves_sure_when_model_reflows_first_line_onto_newline() {
        // Regression: the model moved "Sure," onto its own line while
        // polishing the rest; stripping it deleted the user's opener.
        let input = "Sure, I will send it tomorrow.";
        let output = "Sure\nI\u{2019}ll send it tomorrow.";
        assert_eq!(validate_ai_output(output, input).unwrap(), output);
    }

    #[test]
    fn validate_strips_explicit_here_is_wrapper_when_output_is_not_the_input() {
        // The preamble stripper still removes genuine explicit wrappers —
        // with or without a "Sure," lead-in — on transformed payloads.
        assert_eq!(
            validate_ai_output("Here is the fixed text:\nMeet me at 4.", "meet me at four")
                .unwrap(),
            "Meet me at 4."
        );
        assert_eq!(
            validate_ai_output(
                "Sure, here is the polished text:\nMeet me at 4.",
                "meet me at four"
            )
            .unwrap(),
            "Meet me at 4."
        );
    }

    #[test]
    fn validate_identity_guard_does_not_bypass_refusal_check() {
        let refusal = "I'm sorry, I can't help with that.";
        let error = validate_ai_output(refusal, refusal).unwrap_err();
        assert!(matches!(error, AiProviderError::BadResponse));
    }

    #[test]
    fn validate_preserves_input_first_line_even_when_rest_is_polished() {
        // Regression: the model echoed the input's literal first line ("Sure")
        // and polished only the rest. Full-output identity equality missed
        // this — the line is input content, so it must survive.
        let input = "Sure\nI will send it tomorrow.";
        let output = "Sure\nI\u{2019}ll send it tomorrow.";
        assert_eq!(validate_ai_output(output, input).unwrap(), output);
    }

    #[test]
    fn validate_first_line_guard_is_case_insensitive_on_echoed_preamble() {
        // A case-fiddled echo of the input's first line is still user content.
        let input = "SURE\nSend it tomorrow.";
        let output = "sure\nSend it tomorrow.";
        assert_eq!(validate_ai_output(output, input).unwrap(), output);
    }

    #[test]
    fn sanitize_drops_control_characters_but_keeps_newline_and_tab() {
        // Cc controls (NUL, SOH, BEL, US) are stripped; \n and \t survive because
        // they are matched before the is_control() branch. A large input keeps
        // the length cap from clipping this short fixture.
        let input = "x".repeat(8192);
        let dirty = "hello\0\x01world\x07bell\ttab\nnewline\x1funit";
        let (cleaned, truncated) = sanitize_ai_output(dirty, input.len());
        assert!(!truncated);
        assert_eq!(cleaned, "helloworldbell\ttab\nnewlineunit");
    }

    #[test]
    fn sanitize_strips_bidi_override_characters() {
        let input = "x".repeat(8192);
        // U+202E (RLO) embedded in prose — the classic Trojan-Source override —
        // is removed.
        let (cleaned, truncated) = sanitize_ai_output("ab\u{202E}cd", input.len());
        assert!(!truncated);
        assert_eq!(cleaned, "abcd");
        // The full Cf override set is stripped to nothing.
        let battery = "\u{061C}\u{200E}\u{200F}\u{202A}\u{202B}\u{202C}\u{202D}\u{202E}\u{2066}\u{2067}\u{2068}\u{2069}";
        let (cleaned, truncated) = sanitize_ai_output(battery, input.len());
        assert!(!truncated);
        assert_eq!(cleaned, "");
        // Boundary: zero-width joiner (U+200D) is load-bearing for scripts/emoji
        // and is intentionally NOT a bidi override — it must be preserved.
        let (cleaned, truncated) = sanitize_ai_output("a\u{200D}b", input.len());
        assert!(!truncated);
        assert_eq!(cleaned, "a\u{200D}b");
    }

    #[test]
    fn sanitize_collapses_crlf_and_lone_cr_to_newline() {
        let input = "x".repeat(8192);
        // CRLF collapses to a single newline (the \n is consumed with the \r).
        let (cleaned, truncated) = sanitize_ai_output("line1\r\nline2", input.len());
        assert!(!truncated);
        assert_eq!(cleaned, "line1\nline2");
        // A lone CR becomes a newline.
        let (cleaned, truncated) = sanitize_ai_output("line1\rline2", input.len());
        assert!(!truncated);
        assert_eq!(cleaned, "line1\nline2");
        // A plain newline passes through verbatim.
        let (cleaned, truncated) = sanitize_ai_output("line1\nline2", input.len());
        assert!(!truncated);
        assert_eq!(cleaned, "line1\nline2");
    }

    #[test]
    fn sanitize_reports_truncation_for_short_input_over_floor() {
        let input_len = 1;
        let cap = 4096;
        let overflow = "X".repeat(cap + 1);
        let (cleaned, truncated) = sanitize_ai_output(&overflow, input_len);
        assert!(truncated);
        assert_eq!(cleaned.len(), cap);
        assert_eq!(cleaned, &overflow[..cap]);
    }

    #[test]
    fn sanitize_reports_truncation_for_much_larger_output() {
        let input_len = 8192;
        let cap = input_len * 4;
        let overflow = "X".repeat(cap * 4);
        let (cleaned, truncated) = sanitize_ai_output(&overflow, input_len);
        assert!(truncated);
        assert_eq!(cleaned.len(), cap);
        assert_eq!(cleaned, &overflow[..cap]);
    }

    #[test]
    fn sanitize_preserves_utf8_boundary_at_multibyte_cap() {
        let input_len = 1024;
        let cap = 4096;
        let prefix = "a".repeat(cap - 1);
        let overflow = format!("{prefix}éafter");
        let (cleaned, truncated) = sanitize_ai_output(&overflow, input_len);
        assert!(truncated);
        assert_eq!(cleaned.len(), cap - 1);
        assert_eq!(cleaned, prefix);
        assert!(cleaned.is_char_boundary(cleaned.len()));
    }

    #[test]
    fn sanitize_enforces_length_cap_relative_to_input() {
        // cap = input_byte_len * 4, floored at AI_OUTPUT_MIN_TOKEN_CAP * 4
        // (1024 * 4 = 4096). A large input makes the 4x ceiling dominate so the
        // cap is predictable; a single-byte overflow char truncates exactly to
        // it with a whole-char prefix (no multi-byte split).
        let input_len = 8192;
        let cap = input_len * 4; // 32768 > 4096 floor
        let overflow = "X".repeat(cap * 4);
        let (cleaned, truncated) = sanitize_ai_output(&overflow, input_len);
        assert!(truncated);
        assert_eq!(cleaned.len(), cap);
        assert_eq!(cleaned, &overflow[..cap]);
    }
}
