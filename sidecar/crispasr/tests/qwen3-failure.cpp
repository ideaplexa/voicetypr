// Run the actual pinned adapter against a deterministic failing C API. No model,
// GPU or synthetic-speech accuracy claim is involved in this fault-injection test.
#include "qwen3_asr.h"
#include <cstdlib>
#include <cstring>
#include <cstdio>
#include <stdexcept>
#include "core/gguf_loader.h"

// Metadata discovery is irrelevant to these injected graph failures.
namespace core_gguf {
gguf_context* open_metadata(const char*) { return nullptr; }
std::string kv_str(gguf_context*, const char*, const char* fallback) { return fallback; }
}

struct qwen3_asr_context {};
enum class Fault { Encoder, Prefill, Decoder };
static Fault fault;
static int failures;
static float* floats(int count) { return static_cast<float*>(std::calloc(count, sizeof(float))); }
extern "C" {
qwen3_asr_context_params qwen3_asr_context_default_params() { return {1, 0, false, false}; }
qwen3_asr_context* qwen3_asr_init_from_file(const char*, qwen3_asr_context_params) { return new qwen3_asr_context; }
void qwen3_asr_free(qwen3_asr_context* ctx) { delete ctx; }
bool qwen3_asr_is_raon_speech(qwen3_asr_context*) { return false; }
float* qwen3_asr_raon_encode(qwen3_asr_context*, const float*, int, int*, int*) { return nullptr; }
float* qwen3_asr_compute_mel(qwen3_asr_context*, const float*, int, int* mels, int* time) { *mels = *time = 1; return floats(1); }
float* qwen3_asr_run_encoder(qwen3_asr_context*, const float*, int, int, int* n, int* dim) {
    if (fault == Fault::Encoder) { ++failures; return nullptr; }
    *n = *dim = 1; return floats(1);
}
int32_t* qwen3_asr_tokenize(qwen3_asr_context*, const char* text, int* count) {
    *count = 1;
    auto* ids = static_cast<int32_t*>(std::malloc(sizeof(int32_t)));
    *ids = std::strcmp(text, "<|im_end|>") == 0 || std::strcmp(text, "<|endoftext|>") == 0 ? 2 : 0;
    return ids;
}
const char* qwen3_asr_token_text(qwen3_asr_context*, int id) { return id == 1 ? "word" : ""; }
float* qwen3_asr_embed_tokens(qwen3_asr_context*, const int32_t*, int n) { return floats(n); }
bool qwen3_asr_kv_init(qwen3_asr_context*, int) { return true; }
void qwen3_asr_kv_reset(qwen3_asr_context*) {}
float* qwen3_asr_run_llm_kv(qwen3_asr_context*, const float*, int n, int past, int* out_n, int* vocab) {
    if ((fault == Fault::Prefill && past == 0) || (fault == Fault::Decoder && past != 0)) { ++failures; return nullptr; }
    *out_n = n; *vocab = 3;
    auto* logits = floats(n * 3);
    logits[(n - 1) * 3 + (past == 0 ? 1 : 2)] = 1;
    return logits;
}
}

#include "../src/qwen3_adapter.cpp"

int main() {
    int failed = 0;
    for (auto stage : {Fault::Encoder, Fault::Prefill, Fault::Decoder}) {
        fault = stage;
        failures = 0;
        whisper_params params{.best_of = 1};
        params.model = "missing-test-metadata.gguf";
        Qwen3Backend backend;
        if (!backend.init(params)) return 2;
        Qwen3RealtimeSession session(&backend, params);
        std::vector<float> audio(16'000, 0.1f);
        bool threw = false, finalized = false;
        try { session.append(audio.data(), static_cast<int>(audio.size()), true, [&](const std::string&, bool final) { finalized |= final; }); }
        catch (const std::runtime_error&) { threw = true; }
        if (!threw || finalized || failures != 1) {
            std::fprintf(stderr, "Native failure accepted as success at stage %d\n", static_cast<int>(stage));
            ++failed;
        }
    }
    return failed ? 1 : 0;
}
