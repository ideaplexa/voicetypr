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
}

// Allows roughly 4x the input byte length as output. At about 4 bytes per
// token, that output byte budget is approximately input_byte_len tokens.
pub const AI_OUTPUT_MIN_TOKEN_CAP: u32 = 1024;

pub fn output_token_cap_for_input(input_byte_len: usize) -> u32 {
    let estimated_tokens = input_byte_len.max(AI_OUTPUT_MIN_TOKEN_CAP as usize);
    estimated_tokens.min(u32::MAX as usize) as u32
}

#[cfg(test)]
mod tests {
    use super::{output_token_cap_for_input, AI_OUTPUT_MIN_TOKEN_CAP};

    #[test]
    fn output_token_cap_tracks_input_bytes_with_floor() {
        assert_eq!(output_token_cap_for_input(8_000), 8_000);
        assert_eq!(output_token_cap_for_input(10), AI_OUTPUT_MIN_TOKEN_CAP);
    }
}
