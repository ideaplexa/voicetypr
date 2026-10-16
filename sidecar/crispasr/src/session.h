#pragma once
#include "crispasr_backend.h"
#include "whisper_params.h"
#include "json.hpp"
#include <functional>
#include <optional>

namespace voicetypr {
using Json = nlohmann::json;
using Emit = std::function<void(const Json&)>;

class Session {
public:
    explicit Session(Emit emit) : emit_(std::move(emit)) {}
    void dispatch(const Json& request);
private:
    void load(const Json& request);
    void start(const Json& request);
    void append(const Json& request);
    void finish(const Json& request);
    void reset();
    void require_session(const Json& request) const;
    void partial(uint64_t request_id, const std::string& text, bool committed);
    std::vector<crispasr_segment> transcribe(const std::vector<float>& samples);
    void reply(const Json& request, Json response) const;

    Emit emit_;
    std::unique_ptr<CrispasrBackend> backend_;
    std::unique_ptr<CrispasrRealtimeSession> realtime_;
    // Specifying this member skips whisper_params' Whisper-only default
    // function call. We use the real Parakeet/Qwen adapters without linking a
    // second, unused Whisper engine or the entire upstream model catalog.
    whisper_params params_{.best_of = 1};
    std::string model_;
    std::string backend_name_;
    std::string load_key_;
    std::optional<uint64_t> session_;
    bool batch_ = false;
    bool stream_retry_ = false;
    size_t received_ = 0;
    size_t last_preview_ = 0;
    std::vector<float> audio_;
    std::string final_text_;
};
}
