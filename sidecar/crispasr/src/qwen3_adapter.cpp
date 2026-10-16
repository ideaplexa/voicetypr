// The pinned CLI adapter turns failed native graphs into empty/partial success.
// Check the C API results at this C++ boundary instead, including function
// pointers passed into the greedy decoder. Rust discards the process on error,
// so neither a partial final nor allocations abandoned by upstream can survive.
// Keep this translation unit isolated: upstream source and C API stay unmodified.
#include "qwen3_asr.h"
#include <stdexcept>

namespace voicetypr_qwen3 {
template<class Result> Result require(Result result) {
    if (!result) throw std::runtime_error("inference_failed");
    return result;
}
float* mel(qwen3_asr_context* ctx, const float* samples, int n, int* mels, int* time) {
    return require(qwen3_asr_compute_mel(ctx, samples, n, mels, time));
}
float* encoder(qwen3_asr_context* ctx, const float* mel, int mels, int time, int* n, int* dim) {
    return require(qwen3_asr_run_encoder(ctx, mel, mels, time, n, dim));
}
float* embeddings(qwen3_asr_context* ctx, const int32_t* ids, int n) {
    return require(qwen3_asr_embed_tokens(ctx, ids, n));
}
bool cache(qwen3_asr_context* ctx, int n) {
    return require(qwen3_asr_kv_init(ctx, n));
}
float* decoder(qwen3_asr_context* ctx, const float* embeds, int n, int past, int* out_n, int* vocab) {
    return require(qwen3_asr_run_llm_kv(ctx, embeds, n, past, out_n, vocab));
}
int32_t* tokenize(qwen3_asr_context* ctx, const char* text, int* n) {
    auto* ids = qwen3_asr_tokenize(ctx, text, n);
    // Empty prefixes legitimately tokenize to no tokens. A nonempty prompt may
    // not silently disappear after an allocation/tokenizer failure.
    return text && *text ? require(ids) : ids;
}
}

#define qwen3_asr_compute_mel voicetypr_qwen3::mel
#define qwen3_asr_run_encoder voicetypr_qwen3::encoder
#define qwen3_asr_embed_tokens voicetypr_qwen3::embeddings
#define qwen3_asr_kv_init voicetypr_qwen3::cache
#define qwen3_asr_run_llm_kv voicetypr_qwen3::decoder
#define qwen3_asr_tokenize voicetypr_qwen3::tokenize
#include "crispasr_backend_qwen3.cpp"
