use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProvider {
    pub id: String,
    pub label: String,
    pub status: String,
    pub requires_api_key: bool,
    pub supports_base_url: bool,
    pub supports_reasoning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiModel {
    pub provider_id: String,
    pub model_id: String,
    pub label: String,
    pub recommended: bool,
}

#[derive(Debug, Clone)]
pub struct AiPolishRequest {
    pub provider_id: String,
    pub model_id: String,
    pub reasoning_level: Option<String>,
    pub fast_mode: bool,
    pub input_text: String,
    pub prompt: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiPolishResult {
    pub output_text: String,
    pub provider_id: String,
    pub model_id: String,
    pub duration_ms: u64,
    /// True when a fast-tier request was refused by the provider and the
    /// result came from the standard-speed retry.
    pub fast_mode_fallback: bool,
}

// Visible output budget: ~2x the input in tokens at ~4 bytes per token,
// floored so very short dictations still get a usable rewrite. The executor's
// byte-level sanitizer ceiling (4x input bytes, min 4096) stays strictly
// above this cap, so the provider token limit is always hit first and the
// truncation guard can fire before the sanitizer clips.
pub const AI_OUTPUT_MIN_TOKEN_CAP: u32 = 1024;

/// Thinking allowance added on top of the visible cap for a resolved effort.
/// Every value is strictly above the largest thinking budget any adapter can
/// send for that level (genai legacy budgets: Anthropic 1024/8000/24000 and
/// Gemini 2.5 1000/8000/24000 for low/medium/high), so max_tokens always
/// covers thinking plus visible output. none/minimal also carry an allowance:
/// adaptive thinking and budget-free effort modes still burn tokens.
pub fn thinking_token_allowance(effort: Option<&str>) -> u32 {
    match effort {
        Some("none") | None => 0,
        Some("minimal") => 1024,
        Some("low") => 2048,
        Some("high") => 25_600,
        // medium and any unlisted value: the resolver caps inputs to the
        // catalog levels, and an unhandled arm must never under-budget.
        Some(_) => 9216,
    }
}

/// Total output token cap for a request: the visible budget plus the thinking
/// allowance for the resolved effort, clamped to the model's catalog output
/// limit when known.
pub fn output_token_cap_for_input(
    input_byte_len: usize,
    thinking_allowance: u32,
    output_limit: Option<u64>,
) -> u32 {
    let estimated_tokens = input_byte_len.div_ceil(4).saturating_mul(2).max(256);
    let total = estimated_tokens.saturating_add(thinking_allowance as usize);
    let limit = output_limit.unwrap_or(u64::from(u32::MAX));
    total.min(u64::from(u32::MAX).min(limit) as usize) as u32
}

#[cfg(test)]
mod tests {
    use super::{output_token_cap_for_input, thinking_token_allowance};

    #[test]
    fn output_token_cap_tracks_input_tokens_with_floor() {
        assert_eq!(output_token_cap_for_input(8_000, 0, None), 4_000);
        assert_eq!(output_token_cap_for_input(8_001, 0, None), 4_002);
        assert_eq!(output_token_cap_for_input(10, 0, None), 256);
    }

    #[test]
    fn output_token_cap_adds_thinking_allowance() {
        assert_eq!(output_token_cap_for_input(8_000, 2_048, None), 6_048);
    }

    #[test]
    fn output_token_cap_clamps_to_catalog_output_limit() {
        // The gemini-2.5-flash 65,536 overflow: a large input must clamp to
        // the catalog output limit rather than exceed it.
        assert_eq!(
            output_token_cap_for_input(200_000, 9_216, Some(65_536)),
            65_536
        );
    }

    #[test]
    fn thinking_allowance_stays_above_every_sent_budget() {
        // genai legacy budgets: Anthropic 1024/8000/24000, Gemini 1000/8000/24000.
        for (effort, sent_budget) in [("low", 1024_u32), ("medium", 8000), ("high", 24000)] {
            assert!(
                thinking_token_allowance(Some(effort)) > sent_budget,
                "{effort} allowance must exceed the {sent_budget} budget"
            );
        }
    }
}
