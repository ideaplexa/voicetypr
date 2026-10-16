#include "session.h"
#include "audio.h"
#include "core/audio_chunking.h"
#include "ggml-backend.h"
#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <filesystem>
#include <stdexcept>

std::unique_ptr<CrispasrBackend> crispasr_make_parakeet_backend();
std::unique_ptr<CrispasrBackend> crispasr_make_qwen3_backend();

namespace voicetypr {
namespace {
std::string text_of(const std::vector<crispasr_segment>& segments) {
    std::string text;
    for (const auto& segment : segments) {
        if (segment.text.empty()) continue;
        if (!text.empty() && text.back() != ' ' && segment.text.front() != ' ') text += ' ';
        text += segment.text;
    }
    return text;
}
}

void Session::reply(const Json& request, Json response) const {
    response["id"] = request.at("id");
    emit_(response);
}

void Session::dispatch(const Json& request) {
    const auto type = request.at("type").get<std::string>();
    if (type == "status") {
        reply(request, {{"type", "status"}, {"protocol", 1}, {"loaded_model", model_.empty() ? Json(nullptr) : Json(model_)}});
    } else if (type == "load_model") load(request);
    else if (type == "start_stream") start(request);
    else if (type == "audio_chunk") append(request);
    else if (type == "finalize_stream") finish(request);
    else if (type == "cancel_stream") {
        require_session(request);
        reset();
        reply(request, {{"type", "ok"}});
    } else if (type == "shutdown") {
        reset();
        backend_.reset();
        reply(request, {{"type", "ok"}});
    } else throw std::invalid_argument("unknown_command");
}

void Session::load(const Json& request) {
    if (session_) throw std::invalid_argument("stream_busy");
    const auto path = request.at("model_path").get<std::string>();
    if (!std::filesystem::is_regular_file(std::filesystem::u8path(path))) throw std::invalid_argument("model_not_found");
    const auto backend = request.at("backend").get<std::string>();
    if (backend != "parakeet" && backend != "qwen3") throw std::invalid_argument("unsupported_backend");
    const bool gpu = request.value("gpu", true);
    const auto threads = std::clamp(request.value("threads", 4), 1, 64);
    const auto key = path + "|" + backend + "|" + std::to_string(gpu) + "|" + std::to_string(threads);
    if (backend_ && key == load_key_) {
        model_ = request.at("model").get<std::string>();
        reply(request, {{"type", "loaded"}, {"reused", true}});
        return;
    }
    backend_.reset();
    model_.clear();
    load_key_.clear();
    if (gpu) {
        // A Vulkan loader alone is not a GPU. Refuse silent native CPU fallback
        // here so Rust selects the independent CPU runtime and its slower cadence.
        ggml_backend_load_all();
        auto device = ggml_backend_dev_by_type(GGML_BACKEND_DEVICE_TYPE_GPU);
        if (!device) device = ggml_backend_dev_by_type(GGML_BACKEND_DEVICE_TYPE_IGPU);
        if (!device) throw std::invalid_argument("gpu_unavailable");
        auto probe = ggml_backend_dev_init(device, nullptr);
        if (!probe) throw std::invalid_argument("gpu_unavailable");
        ggml_backend_free(probe);
    }
    params_.model = path;
    params_.n_threads = threads;
    params_.use_gpu = gpu;
    params_.gpu_backend = gpu ? "" : "cpu";
    params_.no_prints = true;
    params_.language = "auto";
    params_.vad = false;
    params_.beam_size = 1;
    auto candidate = backend == "parakeet" ? crispasr_make_parakeet_backend() : crispasr_make_qwen3_backend();
    if (!candidate->init(params_)) throw std::invalid_argument("model_load_failed");
    candidate->warmup();
    if (backend == "qwen3") {
        // Qwen's upstream warmup is a no-op. Touch its encoder and short decoder
        // graph while the app is preloading, with a bounded token budget. This
        // silence is initialization work, never an accuracy/performance fixture.
        auto warm = params_;
        warm.max_new_tokens = 8;
        const std::vector<float> silence(sample_rate, 0.0f);
        candidate->transcribe(silence.data(), static_cast<int>(silence.size()), 0, warm);
    }
    backend_ = std::move(candidate);
    backend_name_ = backend;
    model_ = request.at("model").get<std::string>();
    load_key_ = key;
    reply(request, {{"type", "loaded"}, {"reused", false}});
}

void Session::start(const Json& request) {
    if (!backend_) throw std::invalid_argument("model_not_loaded");
    if (session_) throw std::invalid_argument("stream_busy");
    reset();
    params_.language = request.value("language", std::string("auto"));
    batch_ = request.value("mode", std::string("recording")) == "batch";
    if (backend_name_ == "qwen3") {
        // The 160 ms upstream schedule repeatedly re-encodes accumulated audio.
        // Real speech testing could not sustain it on CPU or mobile-GPU Q8.
        // Coarser supported steps preserve the recipe while keeping up with capture.
        const int step = request.value("step_ms", params_.use_gpu && !batch_ ? 640 : 2000);
        if (step < 80 || step > 2000) throw std::invalid_argument("invalid_request");
        const auto value = std::to_string(step);
#ifdef _WIN32
        _putenv_s("CRISPASR_QWEN3_STREAM_STEP_MS", value.c_str());
#else
        setenv("CRISPASR_QWEN3_STREAM_STEP_MS", value.c_str(), 1);
#endif
    }
    session_ = request.at("session_id").get<uint64_t>();
    if (!batch_ && backend_name_ == "qwen3") {
        realtime_ = backend_->create_realtime_session(params_);
        if (!realtime_) { reset(); throw std::invalid_argument("stream_unavailable"); }
    }
    reply(request, {{"type", "ok"}});
}

void Session::require_session(const Json& request) const {
    if (!session_ || request.at("session_id").get<uint64_t>() != *session_)
        throw std::invalid_argument("session_mismatch");
}

void Session::partial(uint64_t request_id, const std::string& text, bool committed) {
    emit_({{"type", "partial"}, {"id", request_id}, {"session_id", *session_},
        {"committed", committed ? text : ""}, {"tentative", committed ? "" : text}});
}

void Session::append(const Json& request) {
    require_session(request);
    auto samples = decode_audio(request.at("pcm").get<std::string>());
    const size_t limit = sample_rate * (batch_ ? 3600 : 120);
    if (samples.size() > limit - received_) { reset(); throw std::invalid_argument("stream_limit"); }
    received_ += samples.size();
    const auto id = request.at("id").get<uint64_t>();
    if (realtime_) {
        if (!samples.empty() && !realtime_->append(samples.data(), static_cast<int>(samples.size()), false,
            [this, id](const std::string& text, bool) { partial(id, text, true); })) {
            reset();
            throw std::invalid_argument("decode_failed");
        }
    } else {
        audio_.insert(audio_.end(), samples.begin(), samples.end());
        // Full-context tentative preview avoids freezing early TDT mistakes or
        // sliding away the language context. At most one decode runs at a time;
        // Rust bounds ingress backlog and falls back to complete-file decoding.
        if (!batch_ && received_ >= sample_rate && received_ - last_preview_ >= sample_rate) {
            partial(id, text_of(transcribe(audio_)), false);
            last_preview_ = received_;
        }
    }
    reply(request, {{"type", "ok"}, {"samples", received_}});
}

std::vector<crispasr_segment> Session::transcribe(const std::vector<float>& samples) {
    if (samples.empty()) return {};
    if (backend_name_ == "parakeet")
        return backend_->transcribe(samples.data(), static_cast<int>(samples.size()), 0, params_);
    // Use upstream's bounded, energy-minimum chunker for offline R2T2, just as
    // its session API does. This does not load a VAD or drop any audio ranges.
    std::vector<crispasr_segment> all;
    for (const auto& [begin, end] : audio_chunking::split_at_energy_minima(samples.data(), samples.size(), sample_rate * 30, sample_rate * 5)) {
        auto segments = backend_->transcribe(samples.data() + begin, static_cast<int>(end - begin), static_cast<int64_t>(begin * 100 / sample_rate), params_);
        // R2T2 Q4 can return an empty offline Auto result for real speech while
        // its native streaming recipe succeeds on identical PCM. Retry through
        // that recipe once, without inventing/forcing a language. Successful
        // offline results (including Q8) keep their fast single-decode path.
        if ((params_.language.empty() || params_.language == "auto") && text_of(segments).empty()
            && std::any_of(samples.begin() + begin, samples.begin() + end, [](float sample) { return sample != 0; })) {
            auto retry = backend_->create_realtime_session(params_);
            if (retry) {
                std::string text;
                bool finalized = false;
                if (!retry->append(samples.data() + begin, static_cast<int>(end - begin), true,
                    [&text, &finalized](const std::string& result, bool final) {
                        if (final) { text = result; finalized = true; }
                    }) || !finalized) throw std::invalid_argument("finalize_failed");
                segments = {crispasr_segment{.text = text, .t0 = -1, .t1 = -1}};
                stream_retry_ = true;
            }
        }
        all.insert(all.end(), std::make_move_iterator(segments.begin()), std::make_move_iterator(segments.end()));
    }
    return all;
}

void Session::finish(const Json& request) {
    require_session(request);
    const auto started = std::chrono::steady_clock::now();
    Json segments = Json::array();
    if (realtime_) {
        const float empty = 0;
        bool finalized = false;
        if (!realtime_->append(&empty, 0, true, [this, &finalized](const std::string& text, bool final) {
            if (final) { final_text_ = text; finalized = true; }
        }) || !finalized) { reset(); throw std::invalid_argument("finalize_failed"); }
    } else {
        auto result = transcribe(audio_);
        final_text_ = text_of(result);
        for (const auto& segment : result) {
            segments.push_back({{"text", segment.text},
                {"start_ms", segment.t0 < 0 ? Json(nullptr) : Json(segment.t0 * 10)},
                {"end_ms", segment.t1 < 0 ? Json(nullptr) : Json(segment.t1 * 10)}});
        }
    }
    const auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now() - started).count();
    reply(request, {{"type", "final"}, {"session_id", *session_}, {"text", final_text_},
        {"segments", segments}, {"samples", received_}, {"processing_ms", ms}, {"stream_retry", stream_retry_},
        {"language", params_.language.empty() || params_.language == "auto" ? Json(nullptr) : Json(params_.language)}});
    reset();
}

void Session::reset() {
    realtime_.reset();
    session_.reset();
    audio_.clear();
    audio_.shrink_to_fit();
    final_text_.clear();
    stream_retry_ = false;
    received_ = last_preview_ = 0;
}
}
